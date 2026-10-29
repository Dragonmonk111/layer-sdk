//! claw-checkpoint — Merkle-roots each claw-relay channel's event window
//! (JC-TRANSCRIPT-ANCHOR-v1) and anchors the root on-chain via the
//! moultbook contract, then posts a signed receipt event back to the
//! channel. Sovereign Rust replacement for ops/buzz/transcript-anchor.sh.
//!
//! Committed-state discipline (from the script): "broadcast OK" is never
//! trusted — after `bls-relayer jc-execute` we smart-query the moultbook
//! contract (`list_by_author`) until OUR commitment reads back, learn its
//! entry id, and only then advance the cursor (`next_from_ts = to_ts`)
//! and post the receipt. On any failure the cursor stays put and the next
//! pass retries the same window.

use claw_checkpoint::*;
use k256::schnorr::SigningKey;
use std::path::PathBuf;
use std::time::Duration;
use tracing::{error, info, warn};

const USAGE: &str = "\
claw-checkpoint — anchor relay channel transcript windows on-chain

  --relay <ws url>             claw-relay endpoint      (default ws://127.0.0.1:7070)
  --channels \"name:uuid ...\"   channels to anchor, space-separated  (required)
  --moultbook <addr>           moultbook contract address            (required)
  --author <juno1 addr>        author the DAO key posts as — needed for
                               committed-state verification (list_by_author) (required)
  --jc-key-file <path>         64-hex privkey file — contents forwarded to
                               bls-relayer as JUNOCLAW_KEY_HEX env (never argv).
                               Its juno1 address MUST equal --author or the anchor
                               lands under the wrong author and list_by_author
                               can never verify it
  --api <url>                  chain REST API            (default http://127.0.0.1:8082)
  --bls-relayer <path>         bls-relayer binary        (default: bls-relayer on PATH)
  --state-dir <dir>            per-channel anchor-*.json (default ./checkpoint-state)
  --lag-secs <n>               anchor up to now - lag    (default 300)
  --bootstrap-hours <n>        first run covers last n h (default 24)
  --max-size <n>               content-byte cap          (default 60000, < 65536)
  --checkpoint-key-file <path> 32-byte-hex BIP-340 key — sign the checkpoint payload
  --receipt-key-file <path>   nsec-hex key — publish a receipt EVENT to the channel
  --grpc <host:port>           JunoClaw gRPC forwarded to bls-relayer --layer-grpc
  --once                       single pass then exit (cron mode)
  --interval-secs <n>          daemon loop every n secs";

struct Args {
    relay: String,
    channels: Vec<(String, String)>,
    moultbook: String,
    author: String,
    api: String,
    bls_relayer: String,
    state_dir: PathBuf,
    lag_secs: u64,
    bootstrap_hours: u64,
    max_size: u64,
    once: bool,
    interval_secs: Option<u64>,
    receipt_key_file: Option<PathBuf>,
    checkpoint_key_file: Option<PathBuf>,
    jc_key_file: Option<PathBuf>,
    grpc: Option<String>,
}

fn opt<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .map(String::as_str)
}

fn has(args: &[String], name: &str) -> bool {
    args.iter().any(|a| a == name)
}

fn parse_u64(args: &[String], name: &str, default: u64) -> Result<u64, String> {
    opt(args, name)
        .map(|s| s.parse::<u64>().map_err(|_| format!("bad {name}: {s:?}")))
        .transpose()
        .map(|o| o.unwrap_or(default))
}

fn parse_args() -> Result<Args, String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let relay = opt(&args, "--relay")
        .unwrap_or("ws://127.0.0.1:7070")
        .to_string();
    let channels_raw =
        opt(&args, "--channels").ok_or("--channels \"name:uuid ...\" is required")?;
    let mut channels = Vec::new();
    for pair in channels_raw.split_whitespace() {
        let (name, uuid) = pair
            .split_once(':')
            .ok_or_else(|| format!("bad channel pair {pair:?} — expected name:uuid"))?;
        channels.push((name.to_string(), uuid.to_string()));
    }
    if channels.is_empty() {
        return Err("--channels is empty".into());
    }
    let moultbook = opt(&args, "--moultbook")
        .ok_or("--moultbook <contract addr> is required")?
        .to_string();
    let author = opt(&args, "--author").ok_or(
        "--author <juno1 addr> is required — the moultbook author the DAO key \
         posts as; without it committed-state verification (list_by_author) \
         is impossible",
    )?;
    let api = opt(&args, "--api")
        .unwrap_or("http://127.0.0.1:8082")
        .to_string();
    let bls_relayer = opt(&args, "--bls-relayer")
        .unwrap_or("bls-relayer")
        .to_string();
    let state_dir = PathBuf::from(opt(&args, "--state-dir").unwrap_or("./checkpoint-state"));
    let lag_secs = parse_u64(&args, "--lag-secs", 300)?;
    let bootstrap_hours = parse_u64(&args, "--bootstrap-hours", 24)?;
    let max_size = parse_u64(&args, "--max-size", 60_000)?;
    if max_size >= CONTRACT_MAX_SIZE {
        return Err(format!(
            "--max-size {max_size} must stay under the contract max {CONTRACT_MAX_SIZE}"
        ));
    }
    let once = has(&args, "--once");
    let interval_secs = opt(&args, "--interval-secs")
        .map(|s| s.parse::<u64>().map_err(|_| format!("bad --interval-secs: {s:?}")))
        .transpose()?;
    if !once && interval_secs.is_none() {
        return Err("either --once or --interval-secs <n> is required".into());
    }
    Ok(Args {
        relay,
        channels,
        moultbook,
        author: author.to_string(),
        api,
        bls_relayer,
        state_dir,
        lag_secs,
        bootstrap_hours,
        max_size,
        once,
        interval_secs,
        receipt_key_file: opt(&args, "--receipt-key-file").map(PathBuf::from),
        checkpoint_key_file: opt(&args, "--checkpoint-key-file").map(PathBuf::from),
        jc_key_file: opt(&args, "--jc-key-file").map(PathBuf::from),
        grpc: opt(&args, "--grpc").map(str::to_string),
    })
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// `bls-relayer jc-execute --contract <addr> --msg <json>` — explicit argv
/// only, never a shell string (the msg JSON is passed as an argument).
/// Returns (combined stdout+stderr, exit-status success) — the caller
/// still treats "broadcast OK" as unproven until the smart query reads
/// the entry back.
fn bls_execute(
    relayer: &str,
    contract: &str,
    msg: &str,
    jc_key_file: Option<&std::path::Path>,
    grpc: Option<&str>,
) -> Result<(String, bool), Error> {
    let mut cmd = std::process::Command::new(relayer);
    cmd.arg("jc-execute")
        .arg("--contract")
        .arg(contract)
        .arg("--msg")
        .arg(msg);
    // Signer key via env, never argv — `--jc-key-hex` would expose the
    // scalar in `ps`. We read the file and inject JUNOCLAW_KEY_HEX (the
    // deployed bls-relayer's env). Without it bls-relayer falls back to
    // its embedded deployer key and the anchor lands under
    // juno1dz875zg… — invisible to list_by_author(--author) forever.
    if let Some(kf) = jc_key_file {
        let hex = std::fs::read_to_string(kf)
            .map_err(|e| Error::Config(format!("cannot read --jc-key-file {kf:?}: {e}")))?;
        let hex = hex.trim();
        if hex.len() != 64 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(Error::Config(format!(
                "--jc-key-file {kf:?} must contain exactly 64 hex chars"
            )));
        }
        cmd.env("JUNOCLAW_KEY_HEX", hex);
    }
    if let Some(g) = grpc {
        cmd.arg("--layer-grpc").arg(g);
    }
    let out = cmd.output()?;
    let mut text = String::from_utf8_lossy(&out.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&out.stderr));
    Ok((text, out.status.success()))
}

/// One channel's anchor attempt for this pass. Errors are logged by the
/// caller; a FAILED commit is *not* an error — the cursor simply stays
/// put so the next pass retries the same window.
async fn run_channel(
    a: &Args,
    ck_key: Option<&SigningKey>,
    rc_key: Option<&SigningKey>,
    name: &str,
    uuid: &str,
    now: u64,
    to: u64,
) -> Result<(), Error> {
    let st = ChannelState::load(&a.state_dir, name)?;
    let (from, prev) = match &st {
        Some(s) => (s.next_from_ts, s.last_entry.clone()),
        None => (now.saturating_sub(a.bootstrap_hours * 3600), None),
    };
    if to <= from {
        info!(channel = %name, from, to, "nothing new yet");
        return Ok(());
    }

    let mut relay = RelayClient::connect(&a.relay).await?;
    let events = fetch_window(&mut relay, uuid, from, to, PAGE_LIMIT).await?;
    if events.is_empty() {
        // Empty window: no anchor, but advance the clock cursor so ranges
        // stay uniform and verifiable (same as the script).
        ChannelState {
            next_from_ts: to,
            last_entry: prev,
        }
        .save(&a.state_dir, name)?;
        info!(channel = %name, from, to, "empty window — cursor advanced");
        return Ok(());
    }

    let pkg = anchor_package(&events, Some(a.max_size), Some(to))?;
    let refs: Vec<String> = prev.iter().cloned().collect();
    // Same post msg the script built — visibility is the STRING "public",
    // not a map (serde unit variant; map form fails ExecuteMsg parsing).
    let msg = serde_json::json!({"post": {
        "commitment": pkg.root_b64,
        "content_type": CONTENT_TYPE,
        "size_bytes": pkg.size_bytes,
        "attestation_ref": null,
        "visibility": "public",
        "refs": refs,
    }})
    .to_string();

    let (out, ok) = bls_execute(
        &a.bls_relayer,
        &a.moultbook,
        &msg,
        a.jc_key_file.as_deref(),
        a.grpc.as_deref(),
    )?;
    let txh = extract_txhash(&out);

    // Committed-state verification: find OUR entry by commitment via
    // list_by_author — an anchor is only real once it reads back from
    // state. list_by_author returns entries in ascending HASH order
    // ("moult:<sha256hex>" ids, not chronological), so a single page can
    // silently exclude the fresh entry once the author has more posts
    // than the page limit — paginate via start_after until exhausted.
    // 6 attempts × 5s, same as the script.
    let mut entry = None;
    for _ in 0..6 {
        tokio::time::sleep(Duration::from_secs(5)).await;
        match find_entry_paginated(&a.api, &a.moultbook, &a.author, &pkg.root_b64).await {
            Ok(e) => {
                entry = e;
                if entry.is_some() {
                    break;
                }
            }
            Err(e) => warn!(channel = %name, %e, "smart query failed"),
        }
    }
    let Some(entry) = entry else {
        error!(
            channel = %name,
            from,
            to = pkg.to_ts,
            tx = ?txh,
            relayer_ok = ok,
            tail = %out.lines().last().unwrap_or("").chars().take(160).collect::<String>(),
            "FAILED commit — cursor not advanced, next pass retries"
        );
        return Ok(());
    };

    // Verified in state — advance the cursor to the anchored window's
    // exclusive bound (pkg.to_ts, not the requested `to`, on truncation).
    ChannelState {
        next_from_ts: pkg.to_ts,
        last_entry: Some(entry.clone()),
    }
    .save(&a.state_dir, name)?;

    // Checkpoint signature over the canonical package serialization.
    let canonical = canonical_checkpoint_json(&pkg);
    let sig = match ck_key {
        Some(k) => {
            let s = sign_checkpoint(k, &canonical)?;
            info!(
                channel = %name,
                checkpoint_pubkey = %s.pubkey_hex,
                checkpoint_sig = %s.sig_hex,
                msg_sha256 = %s.msg_sha256_hex,
                "checkpoint signed"
            );
            Some(s)
        }
        None => None,
    };

    // Receipt event back to the channel — only after the entry read back.
    if let Some(rk) = rc_key {
        let content = serde_json::json!({
            "receipt": "JC-TRANSCRIPT-ANCHOR-RECEIPT-v1",
            "channel": name,
            "window": { "from_ts": from, "to_ts": pkg.to_ts },
            "package": serde_json::to_value(&pkg)?,
            "entry_id": entry,
            "txhash": txh,
            "prev_entry": prev,
            "checkpoint": sig,
        })
        .to_string();
        let ev = make_event(rk, uuid, content, now_secs());
        match relay.publish(&ev).await {
            Ok(()) => info!(channel = %name, event_id = %ev.id, "receipt published"),
            Err(e) => warn!(channel = %name, %e, "receipt publish failed"),
        }
    }

    info!(
        channel = %name,
        from,
        to = pkg.to_ts,
        count = pkg.count,
        size_bytes = pkg.size_bytes,
        truncated = pkg.truncated,
        root = %pkg.root_hex,
        entry = %entry,
        tx = ?txh,
        "anchored"
    );
    Ok(())
}

async fn run_pass(
    a: &Args,
    ck_key: Option<&SigningKey>,
    rc_key: Option<&SigningKey>,
) -> Result<(), Error> {
    let now = now_secs();
    let to = now.saturating_sub(a.lag_secs);
    for (name, uuid) in &a.channels {
        if let Err(e) = run_channel(a, ck_key, rc_key, name, uuid, now, to).await {
            error!(channel = %name, %e, "channel pass failed");
        }
    }
    Ok(())
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "claw_checkpoint=info".into()),
        )
        .init();

    if std::env::args().skip(1).any(|a| a == "--help" || a == "-h") {
        println!("{USAGE}");
        return;
    }
    let args = match parse_args() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("error: {e}\n\n{USAGE}");
            std::process::exit(2);
        }
    };

    if let Err(e) = std::fs::create_dir_all(&args.state_dir) {
        eprintln!("error: cannot create --state-dir {}: {e}", args.state_dir.display());
        std::process::exit(1);
    }
    let load = |p: &Option<PathBuf>| -> Option<SigningKey> {
        p.as_ref().map(|path| {
            load_signing_key(path).unwrap_or_else(|e| {
                eprintln!("error: {e}");
                std::process::exit(1);
            })
        })
    };
    let ck_key = load(&args.checkpoint_key_file);
    let rc_key = load(&args.receipt_key_file);

    loop {
        if let Err(e) = run_pass(&args, ck_key.as_ref(), rc_key.as_ref()).await {
            error!(%e, "pass failed");
        }
        if args.once {
            break;
        }
        let interval = args.interval_secs.unwrap_or(300);
        tokio::time::sleep(Duration::from_secs(interval)).await;
    }
}
