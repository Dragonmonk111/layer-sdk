//! cdc-archiver — durable CDC follower for JunoClaw.
//!
//! Consumes `layer.sync.v1.Query/ChangesSince` from any serving node and
//! writes a faithful raw event archive into SQLite. Resume-safe: the last
//! persisted sequence lives in a `meta` table, and the stream reconnects
//! with capped backoff so ISP/host blips don't kill the follower.
//!
//! Usage:
//!   cdc-archiver --grpc 51.195.201.74:9090 --db /data/archive.sqlite3
//!                [--from-seq 0] [--batch 512]
//!
//! Tables:
//!   meta(k,v)                — last_seq, bootstrapped_from
//!   changes(seq,idx,op,...)  — every StateChange, PK(seq,idx) dedupes replays
//!   snapshot(module,bucket,keys,value) — CurrentState dump, only used if the
//!     donor's journal no longer reaches our resume point

use std::time::Duration;

use layer_proto::layer::sync::v1::query_client::QueryClient as SyncQueryClient;
use layer_proto::layer::sync::v1::state_change::Event;
use layer_proto::layer::sync::v1::{
    QueryLatestSequenceRequest, StreamChangesSinceRequest, StreamCurrentStateRequest,
};
use rusqlite::{params, Connection};
use tonic::transport::Channel;

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS meta(k TEXT PRIMARY KEY, v TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS changes(
    seq    INTEGER NOT NULL,
    idx    INTEGER NOT NULL,
    op     TEXT    NOT NULL,
    module TEXT    NOT NULL,
    bucket TEXT    NOT NULL,
    keys   TEXT    NOT NULL,
    value  BLOB,
    PRIMARY KEY(seq, idx)
) WITHOUT ROWID;
CREATE INDEX IF NOT EXISTS idx_changes_mb ON changes(module, bucket);
CREATE TABLE IF NOT EXISTS snapshot(
    module TEXT NOT NULL,
    bucket TEXT NOT NULL,
    keys   TEXT NOT NULL,
    value  BLOB,
    PRIMARY KEY(module, bucket, keys)
) WITHOUT ROWID;
-- Typed projections, folded from `changes` (cursor: meta.typed_seq).
CREATE TABLE IF NOT EXISTS accounts(
    address   TEXT PRIMARY KEY,
    first_seq INTEGER NOT NULL,
    last_seq  INTEGER NOT NULL
) WITHOUT ROWID;
CREATE TABLE IF NOT EXISTS balances(
    address TEXT NOT NULL,
    denom   TEXT NOT NULL,
    amount  TEXT NOT NULL,
    seq     INTEGER NOT NULL,
    PRIMARY KEY(address, denom)
) WITHOUT ROWID;
CREATE TABLE IF NOT EXISTS supply(
    denom  TEXT PRIMARY KEY,
    amount TEXT NOT NULL,
    seq    INTEGER NOT NULL
) WITHOUT ROWID;
CREATE TABLE IF NOT EXISTS contracts(
    address TEXT PRIMARY KEY,
    code_id TEXT,
    data    BLOB,
    seq     INTEGER NOT NULL
) WITHOUT ROWID;
CREATE TABLE IF NOT EXISTS codes(
    code_id TEXT PRIMARY KEY,
    data    BLOB,
    seq     INTEGER NOT NULL
) WITHOUT ROWID;
-- Every write to a wasm module's contract-storage bucket: the raw feed the
-- agent/DAO indexer views are built from.
CREATE TABLE IF NOT EXISTS contract_writes(
    seq     INTEGER NOT NULL,
    idx     INTEGER NOT NULL,
    bucket  TEXT    NOT NULL,
    keys    TEXT    NOT NULL,
    value   BLOB,
    PRIMARY KEY(seq, idx)
) WITHOUT ROWID;
CREATE INDEX IF NOT EXISTS idx_cw_bucket ON contract_writes(bucket);
-- Per-sequence block record folded from node-local meta buckets:
-- _cert/<height> carries the hybrid cert bytes, _app_hash/_state_root the
-- certified state root, _proposer the leader. Certified block history,
-- queryable even while the chain has no module-state traffic.
CREATE TABLE IF NOT EXISTS blocks(
    seq        INTEGER PRIMARY KEY,
    height     INTEGER,
    app_hash   BLOB,
    state_root BLOB,
    proposer   BLOB,
    cert       BLOB
) WITHOUT ROWID;
CREATE INDEX IF NOT EXISTS idx_blocks_height ON blocks(height);
";

fn arg(flag: &str, args: &[String]) -> Option<String> {
    args.windows(2).find(|w| w[0] == flag).map(|w| w[1].clone())
}

fn meta_get(db: &Connection, k: &str) -> Option<String> {
    db.query_row("SELECT v FROM meta WHERE k=?1", params![k], |r| r.get(0))
        .ok()
}

fn meta_set(db: &Connection, k: &str, v: &str) -> rusqlite::Result<()> {
    db.execute(
        "INSERT INTO meta(k,v) VALUES(?1,?2) ON CONFLICT(k) DO UPDATE SET v=?2",
        params![k, v],
    )?;
    Ok(())
}

/// First element of a JSON-array `keys` column, or "".
fn key0(keys_json: &str) -> String {
    serde_json::from_str::<Vec<String>>(keys_json)
        .ok()
        .and_then(|v| v.into_iter().next())
        .unwrap_or_default()
}

/// Best-effort display string for a JSON-encoded amount value.
fn amount_str(value: &[u8]) -> String {
    match serde_json::from_slice::<serde_json::Value>(value) {
        Ok(serde_json::Value::String(s)) => s,
        Ok(v) => v
            .get("amount")
            .map(|a| {
                a.as_str()
                    .map(String::from)
                    .unwrap_or_else(|| a.to_string())
            })
            .unwrap_or_else(|| v.to_string()),
        Err(_) => hex::encode(value),
    }
}

/// Fold one archived event into the typed projection tables. Call inside the
/// same transaction as the raw insert so the two never drift.
fn apply_typed(
    tx: &Connection,
    seq: i64,
    idx: i64,
    op: &str,
    module: &str,
    bucket: &str,
    keys: &str,
    value: Option<&[u8]>,
) -> rusqlite::Result<()> {
    match (module, bucket) {
        ("bank", "balances") => {
            let addr = key0(keys);
            let denom = serde_json::from_str::<Vec<String>>(keys)
                .ok()
                .and_then(|v| v.get(1).cloned())
                .unwrap_or_default();
            tx.execute(
                "INSERT INTO accounts(address,first_seq,last_seq) VALUES(?1,?2,?2)
                 ON CONFLICT(address) DO UPDATE SET last_seq=?2",
                params![addr, seq],
            )?;
            if op == "d" {
                tx.execute(
                    "DELETE FROM balances WHERE address=?1 AND denom=?2",
                    params![addr, denom],
                )?;
            } else if let Some(v) = value {
                tx.execute(
                    "INSERT INTO balances(address,denom,amount,seq) VALUES(?1,?2,?3,?4)
                     ON CONFLICT(address,denom) DO UPDATE SET amount=?3, seq=?4",
                    params![addr, denom, amount_str(v), seq],
                )?;
            }
        }
        ("bank", "supply") => {
            let denom = key0(keys);
            if op == "d" {
                tx.execute("DELETE FROM supply WHERE denom=?1", params![denom])?;
            } else if let Some(v) = value {
                tx.execute(
                    "INSERT INTO supply(denom,amount,seq) VALUES(?1,?2,?3)
                     ON CONFLICT(denom) DO UPDATE SET amount=?2, seq=?3",
                    params![denom, amount_str(v), seq],
                )?;
            }
        }
        ("auth", "accounts") => {
            let addr = key0(keys);
            if op == "d" {
                tx.execute("DELETE FROM accounts WHERE address=?1", params![addr])?;
            } else {
                tx.execute(
                    "INSERT INTO accounts(address,first_seq,last_seq) VALUES(?1,?2,?2)
                     ON CONFLICT(address) DO UPDATE SET last_seq=?2",
                    params![addr, seq],
                )?;
            }
        }
        ("wasm", "contracts") => {
            let addr = key0(keys);
            if op == "d" {
                tx.execute("DELETE FROM contracts WHERE address=?1", params![addr])?;
            } else if let Some(v) = value {
                let code_id = serde_json::from_slice::<serde_json::Value>(v)
                    .ok()
                    .and_then(|j| j.get("code_id").map(|c| c.to_string()));
                tx.execute(
                    "INSERT INTO contracts(address,code_id,data,seq) VALUES(?1,?2,?3,?4)
                     ON CONFLICT(address) DO UPDATE SET code_id=?2, data=?3, seq=?4",
                    params![addr, code_id, v, seq],
                )?;
            }
        }
        ("wasm", "codes") => {
            let id = key0(keys);
            if op == "d" {
                tx.execute("DELETE FROM codes WHERE code_id=?1", params![id])?;
            } else if let Some(v) = value {
                tx.execute(
                    "INSERT INTO codes(code_id,data,seq) VALUES(?1,?2,?3)
                     ON CONFLICT(code_id) DO UPDATE SET data=?2, seq=?3",
                    params![id, v, seq],
                )?;
            }
        }
        ("wasm", _) => {
            // Contract-internal storage: bucket is the contract's storage
            // namespace. Keep the full audit trail for the agent indexer.
            tx.execute(
                "INSERT OR IGNORE INTO contract_writes(seq,idx,bucket,keys,value) VALUES(?1,?2,?3,?4,?5)",
                params![seq, idx, bucket, keys, value],
            )?;
        }
        ("", "_app_hash") => {
            if let Some(v) = value {
                tx.execute(
                    "INSERT INTO blocks(seq,app_hash) VALUES(?1,?2)
                     ON CONFLICT(seq) DO UPDATE SET app_hash=?2",
                    params![seq, v],
                )?;
            }
        }
        ("", "_state_root") => {
            if let Some(v) = value {
                tx.execute(
                    "INSERT INTO blocks(seq,state_root) VALUES(?1,?2)
                     ON CONFLICT(seq) DO UPDATE SET state_root=?2",
                    params![seq, v],
                )?;
            }
        }
        ("", "_proposer") => {
            if let Some(v) = value {
                tx.execute(
                    "INSERT INTO blocks(seq,proposer) VALUES(?1,?2)
                     ON CONFLICT(seq) DO UPDATE SET proposer=?2",
                    params![seq, v],
                )?;
            }
        }
        ("", b) if b.starts_with("_cert/") => {
            let h: i64 = b[6..].parse().unwrap_or(-1);
            if op == "d" {
                tx.execute(
                    "UPDATE blocks SET cert=NULL WHERE height=?1",
                    params![h],
                )?;
            } else if let Some(v) = value {
                tx.execute(
                    "INSERT INTO blocks(seq,height,cert) VALUES(?1,?2,?3)
                     ON CONFLICT(seq) DO UPDATE SET height=?2, cert=?3",
                    params![seq, h, v],
                )?;
            }
        }
        _ => {}
    }
    Ok(())
}

/// Catch typed projections up to the raw archive (after restart or upgrade).
fn backfill_typed(db: &Connection) -> Result<(), Box<dyn std::error::Error>> {
    // Schema v2 added the `blocks` table: wipe projections + cursor once so
    // the whole journal re-folds.
    const SCHEMA_V: &str = "2";
    if meta_get(db, "typed_schema_v").as_deref() != Some(SCHEMA_V) {
        db.execute_batch(
            "DELETE FROM accounts; DELETE FROM balances; DELETE FROM supply;
             DELETE FROM contracts; DELETE FROM codes; DELETE FROM contract_writes;
             DELETE FROM blocks;",
        )?;
        meta_set(db, "typed_seq", "-1")?;
        meta_set(db, "typed_schema_v", SCHEMA_V)?;
    }
    let mut typed_seq: i64 = meta_get(db, "typed_seq")
        .and_then(|v| v.parse().ok())
        .unwrap_or(-1);
    loop {
        let tx = db.unchecked_transaction()?;
        let mut stmt = tx.prepare(
            "SELECT seq,idx,op,module,bucket,keys,value FROM changes
             WHERE seq>?1 ORDER BY seq,idx LIMIT 20000",
        )?;
        let mut rows = stmt.query(params![typed_seq])?;
        let mut n = 0u64;
        while let Some(r) = rows.next()? {
            let seq: i64 = r.get(0)?;
            let value: Option<Vec<u8>> = r.get(6)?;
            apply_typed(
                &tx,
                seq,
                r.get(1)?,
                &r.get::<_, String>(2)?,
                &r.get::<_, String>(3)?,
                &r.get::<_, String>(4)?,
                &r.get::<_, String>(5)?,
                value.as_deref(),
            )?;
            typed_seq = seq;
            n += 1;
        }
        drop(rows);
        drop(stmt);
        meta_set(&tx, "typed_seq", &typed_seq.to_string())?;
        tx.commit()?;
        if n == 0 {
            break;
        }
        eprintln!("[cdc-archiver] typed backfill at seq={typed_seq}");
    }
    Ok(())
}

async fn connect(addr: &str) -> Result<SyncQueryClient<Channel>, tonic::transport::Error> {
    let tls = addr.starts_with("https://");
    let uri = if addr.contains("://") {
        addr.to_string()
    } else {
        format!("http://{addr}")
    };
    let mut ep = Channel::from_shared(uri).expect("bad grpc addr");
    if tls {
        ep = ep
            .tls_config(tonic::transport::ClientTlsConfig::new().with_webpki_roots())
            .expect("tls config");
    }
    ep.connect().await.map(SyncQueryClient::new)
}

/// Dump CurrentState into `snapshot` and return the donor's latest sequence.
async fn bootstrap_snapshot(
    db: &Connection,
    client: &mut SyncQueryClient<Channel>,
) -> Result<u64, Box<dyn std::error::Error>> {
    let mut stream = client
        .current_state(StreamCurrentStateRequest {})
        .await?
        .into_inner();
    let tx = db.unchecked_transaction()?;
    let mut n = 0u64;
    while let Some(w) = stream.message().await? {
        tx.execute(
            "INSERT OR REPLACE INTO snapshot(module,bucket,keys,value) VALUES(?1,?2,?3,?4)",
            params![w.module, w.bucket, serde_json::to_string(&w.keys)?, w.value],
        )?;
        n += 1;
    }
    tx.commit()?;
    eprintln!("[cdc-archiver] snapshot bootstrap: {n} state items");
    let seq = client
        .latest_sequence(QueryLatestSequenceRequest {})
        .await?
        .into_inner()
        .sequence;
    meta_set(db, "bootstrapped_from", &seq.to_string())?;
    Ok(seq)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    let grpc = arg("--grpc", &args).unwrap_or_else(|| "127.0.0.1:9090".into());
    let db_path = arg("--db", &args).unwrap_or_else(|| "archive.sqlite3".into());
    let from_seq: Option<u64> = arg("--from-seq", &args).and_then(|s| s.parse().ok());
    let batch: usize = arg("--batch", &args)
        .and_then(|s| s.parse().ok())
        .unwrap_or(512)
        .max(1);

    let db = Connection::open(&db_path)?;
    db.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;")?;
    db.execute_batch(SCHEMA)?;
    backfill_typed(&db)?;

    let mut last: u64 = meta_get(&db, "last_seq")
        .and_then(|v| v.parse().ok())
        .or(from_seq)
        .unwrap_or(0);
    eprintln!("[cdc-archiver] grpc={grpc} db={db_path} resume_from_seq={last}");

    let mut backoff = Duration::from_secs(1);
    loop {
        match connect(&grpc).await {
            Ok(mut client) => {
                backoff = Duration::from_secs(1);
                let mut stream = match client
                    .changes_since(StreamChangesSinceRequest { sequence: last })
                    .await
                {
                    Ok(r) => r.into_inner(),
                    Err(e) => {
                        eprintln!("[cdc-archiver] changes_since({last}) failed: {e}");
                        match bootstrap_snapshot(&db, &mut client).await {
                            Ok(seq) => {
                                last = seq;
                                meta_set(&db, "last_seq", &last.to_string())?;
                            }
                            Err(be) => eprintln!("[cdc-archiver] snapshot fallback failed: {be}"),
                        }
                        continue;
                    }
                };

                let mut pending = 0usize;
                let mut tx = db.unchecked_transaction()?;
                loop {
                    match stream.message().await {
                        Ok(Some(bw)) => {
                            for (i, ev) in bw.events.iter().enumerate() {
                                let (op, module, bucket, keys, value) = match &ev.event {
                                    Some(Event::WriteState(w)) => (
                                        "w",
                                        w.module.clone(),
                                        w.bucket.clone(),
                                        serde_json::to_string(&w.keys)?,
                                        Some(w.value.clone()),
                                    ),
                                    Some(Event::DeleteState(d)) => (
                                        "d",
                                        d.module.clone(),
                                        d.bucket.clone(),
                                        serde_json::to_string(&d.keys)?,
                                        None,
                                    ),
                                    None => continue,
                                };
                                tx.execute(
                                    "INSERT OR IGNORE INTO changes(seq,idx,op,module,bucket,keys,value) VALUES(?1,?2,?3,?4,?5,?6,?7)",
                                    params![bw.sequence as i64, i as i64, op, module, bucket, keys, value],
                                )?;
                                apply_typed(
                                    &tx,
                                    bw.sequence as i64,
                                    i as i64,
                                    op,
                                    &module,
                                    &bucket,
                                    &keys,
                                    value.as_deref(),
                                )?;
                            }
                            last = bw.sequence;
                            meta_set(&tx, "last_seq", &last.to_string())?;
                            pending += 1;
                            if pending >= batch {
                                tx.commit()?;
                                eprintln!("[cdc-archiver] archived seq={last}");
                                tx = db.unchecked_transaction()?;
                                pending = 0;
                            }
                        }
                        Ok(None) => {
                            tx.commit()?;
                            eprintln!("[cdc-archiver] stream closed at seq={last}; reconnecting");
                            break;
                        }
                        Err(e) => {
                            tx.commit()?;
                            eprintln!("[cdc-archiver] stream error at seq={last}: {e}; reconnecting");
                            break;
                        }
                    }
                }
            }
            Err(e) => {
                eprintln!("[cdc-archiver] connect failed: {e} (retry in {:?})", backoff);
                tokio::time::sleep(backoff).await;
                backoff = (backoff * 2).min(Duration::from_secs(30));
            }
        }
    }
}
