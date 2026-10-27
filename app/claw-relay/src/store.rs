//! redb-backed event store. Two tables:
//!   EVENTS:  id[32] -> canonical event JSON (insert-only)
//!   BY_CHAN: (channel, created_at, id) -> id[32]   (query index)
//!
//! redb is a single-writer, multi-reader embedded store — one file, no
//! Postgres/Redis, which is the whole point of the sovereign relay.

use claw_core::{Event, Filter};
use redb::{Database, ReadableTable, TableDefinition};

const EVENTS: TableDefinition<&[u8; 32], &str> = TableDefinition::new("events");
// (channel_utf8 || 0x00 || created_at_be || id) ordered key -> event id
const BY_CHAN: TableDefinition<&[u8], &[u8; 32]> = TableDefinition::new("by_channel");

fn chan_key(channel: &str, created_at: u64, id: &[u8; 32]) -> Vec<u8> {
    let mut k = Vec::with_capacity(channel.len() + 1 + 8 + 32);
    k.extend_from_slice(channel.as_bytes());
    k.push(0);
    k.extend_from_slice(&created_at.to_be_bytes());
    k.extend_from_slice(id);
    k
}

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error(transparent)]
    Db(#[from] Box<dyn std::error::Error + Send + Sync>),
}

impl From<redb::Error> for StoreError {
    fn from(e: redb::Error) -> Self {
        StoreError::Db(e.into())
    }
}
impl From<redb::DatabaseError> for StoreError {
    fn from(e: redb::DatabaseError) -> Self {
        StoreError::Db(e.into())
    }
}
impl From<redb::TransactionError> for StoreError {
    fn from(e: redb::TransactionError) -> Self {
        StoreError::Db(e.into())
    }
}
impl From<redb::TableError> for StoreError {
    fn from(e: redb::TableError) -> Self {
        StoreError::Db(e.into())
    }
}
impl From<redb::StorageError> for StoreError {
    fn from(e: redb::StorageError) -> Self {
        StoreError::Db(e.into())
    }
}
impl From<redb::CommitError> for StoreError {
    fn from(e: redb::CommitError) -> Self {
        StoreError::Db(e.into())
    }
}

pub struct Store {
    db: Database,
}

impl Store {
    pub fn open(path: &std::path::Path) -> Result<Self, StoreError> {
        let db = Database::create(path)?;
        // create tables up front
        let w = db.begin_write()?;
        {
            let _ = w.open_table(EVENTS)?;
            let _ = w.open_table(BY_CHAN)?;
        }
        w.commit()?;
        Ok(Self { db })
    }

    /// Insert an already-verified event. Idempotent on id.
    pub fn put(&self, ev: &Event) -> Result<bool, StoreError> {
        let id: [u8; 32] = hex::decode(&ev.id)
            .map_err(|_| StoreError::Db("bad id hex".into()))?
            .as_slice()
            .try_into()
            .map_err(|_| StoreError::Db("bad id len".into()))?;
        let json = serde_json::to_string(ev).map_err(|e| StoreError::Db(e.into()))?;
        let w = self.db.begin_write()?;
        {
            let mut t = w.open_table(EVENTS)?;
            if t.get(&id)?.is_some() {
                return Ok(false); // duplicate — dedupe by id
            }
            t.insert(&id, json.as_str())?;
            if let Some(chan) = ev.tag_value("h") {
                let mut idx = w.open_table(BY_CHAN)?;
                idx.insert(chan_key(chan, ev.created_at, &id).as_slice(), &id)?;
            }
        }
        w.commit()?;
        Ok(true)
    }

    /// Query with a single filter (OR of multiple filters is done by the
    /// caller unioning results). Newest-first, `limit` applied.
    pub fn query(&self, f: &Filter) -> Result<Vec<Event>, StoreError> {
        let r = self.db.begin_read()?;
        let events = r.open_table(EVENTS)?;
        let mut out: Vec<Event> = Vec::new();
        let limit = f.limit.unwrap_or(500).min(2000) as usize;

        // fast path: single #h channel filter -> range-scan the index
        let chans = f.tag_values("h");
        if chans.len() == 1 {
            let idx = r.open_table(BY_CHAN)?;
            let lo = chan_key(&chans[0], 0, &[0u8; 32]);
            let hi = chan_key(&chans[0], u64::MAX, &[0xffu8; 32]);
            let mut vals: Vec<Event> = Vec::new();
            for item in idx.range(lo.as_slice()..hi.as_slice())? {
                let (_, gid) = item?;
                if let Some(gv) = events.get(gid.value())? {
                    let ev: Event = serde_json::from_str(gv.value())
                        .map_err(|e| StoreError::Db(e.into()))?;
                    if f.matches(&ev) {
                        vals.push(ev);
                    }
                }
            }
            vals.sort_by_key(|e| e.created_at);
            vals.reverse();
            out.extend(vals.into_iter().take(limit));
            return Ok(out);
        }

        // general path: scan all events (bounded by table size; fine at
        // our scale — thousands, not millions)
        for item in events.iter()? {
            let (_, gv) = item?;
            let ev: Event =
                serde_json::from_str(gv.value()).map_err(|e| StoreError::Db(e.into()))?;
            if f.matches(&ev) {
                out.push(ev);
            }
        }
        out.sort_by_key(|e| e.created_at);
        out.reverse();
        out.truncate(limit);
        Ok(out)
    }

    /// All ids for a channel in (created_at, id) order — the query the
    /// checkpoint signer will use to build anchor windows.
    #[allow(dead_code)]
    pub fn channel_window(
        &self,
        channel: &str,
        from_ts: u64,
        to_ts: u64,
    ) -> Result<Vec<Event>, StoreError> {
        let r = self.db.begin_read()?;
        let idx = r.open_table(BY_CHAN)?;
        let events = r.open_table(EVENTS)?;
        let lo = chan_key(channel, from_ts, &[0u8; 32]);
        let hi = chan_key(channel, to_ts.saturating_sub(1), &[0xffu8; 32]);
        let mut out = Vec::new();
        for item in idx.range(lo.as_slice()..hi.as_slice())? {
            let (_, gid) = item?;
            if let Some(gv) = events.get(gid.value())? {
                out.push(serde_json::from_str(gv.value()).map_err(|e| StoreError::Db(e.into()))?);
            }
        }
        out.sort_by_key(|e: &Event| (e.created_at, e.id.clone()));
        Ok(out)
    }
}
