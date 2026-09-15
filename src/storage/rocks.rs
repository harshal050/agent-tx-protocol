//! [`RocksStore`]: transactional state, undo-journal snapshots and Saga logs.
//!
//! # Concurrency contract
//!
//! Operations on *different* transactions are fully concurrent. Operations on
//! the *same* transaction must be serialized by the caller (the engine holds a
//! per-transaction mutex); commits across transactions are serialized
//! internally for first-committer-wins conflict detection.

use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use dashmap::DashMap;
use rocksdb::{
    ColumnFamily, ColumnFamilyDescriptor, DBCompressionType, Direction, IteratorMode, Options,
    WriteBatch, WriteOptions, DB,
};
use serde::{Deserialize, Serialize};

use super::snapshot::{JournalEntry, RestoreStats, SnapshotId, SnapshotMeta, keys, now_ms};
use crate::errors::{AgentTxError, Result};

pub const CF_DEFAULT: &str = "default";
pub const CF_SNAPSHOTS: &str = "snapshots";
pub const CF_SAGA_LOGS: &str = "saga_logs";

const TAG_TOMBSTONE: u8 = 0;
const TAG_VALUE: u8 = 1;
const VERSION_LEN: usize = 8;
const OVERLAY_HEADER_LEN: usize = 1 + VERSION_LEN;

/// Raw key/value pair read from RocksDB.
type RawEntry = (Box<[u8]>, Box<[u8]>);

/// Tunables for [`RocksStore::open`].
#[derive(Debug, Clone)]
pub struct StoreOptions {
    /// fsync the WAL on every write batch.
    pub sync_writes: bool,
    /// Background compaction/flush threads.
    pub parallelism: i32,
}

impl Default for StoreOptions {
    fn default() -> Self {
        Self {
            sync_writes: false,
            parallelism: std::thread::available_parallelism().map_or(2, |n| n.get() as i32),
        }
    }
}

/// Durable marker for a transaction that has begun but not finished.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TxRecord {
    pub tx_id: String,
    pub agent_id: String,
    pub created_at_ms: u64,
}

/// RocksDB-backed store shared by all transactions.
pub struct RocksStore {
    db: DB,
    options: StoreOptions,
    /// Next undo-journal sequence number per open transaction.
    journal_seq: DashMap<String, u64>,
    /// Monotonic version stamped on committed values.
    commit_version: AtomicU64,
    /// Serializes validate-and-apply of commits.
    commit_lock: Mutex<()>,
}

impl std::fmt::Debug for RocksStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RocksStore")
            .field("path", &self.db.path())
            .field("options", &self.options)
            .finish_non_exhaustive()
    }
}

impl RocksStore {
    /// Opens (creating if needed) the database and its column families.
    pub fn open(path: impl AsRef<Path>, options: StoreOptions) -> Result<Self> {
        let mut db_opts = Options::default();
        db_opts.create_if_missing(true);
        db_opts.create_missing_column_families(true);
        db_opts.increase_parallelism(options.parallelism.max(1));
        db_opts.set_compression_type(DBCompressionType::Lz4);
        db_opts.set_bottommost_compression_type(DBCompressionType::Zstd);

        let cfs = [CF_DEFAULT, CF_SNAPSHOTS, CF_SAGA_LOGS].map(|name| {
            let mut cf_opts = Options::default();
            cf_opts.set_compression_type(DBCompressionType::Lz4);
            ColumnFamilyDescriptor::new(name, cf_opts)
        });
        let db = DB::open_cf_descriptors(&db_opts, path, cfs)?;

        let store = Self {
            db,
            options,
            journal_seq: DashMap::new(),
            commit_version: AtomicU64::new(0),
            commit_lock: Mutex::new(()),
        };
        let version = match store.db.get_cf(store.cf(CF_SNAPSHOTS)?, keys::COMMIT_VERSION)? {
            Some(bytes) => decode_u64(&bytes, "c/commit_version")?,
            None => 0,
        };
        store.commit_version.store(version, Ordering::SeqCst);
        Ok(store)
    }

    /// Returns a handle bound to one transaction.
    pub fn scoped(self: &Arc<Self>, tx_id: &str) -> Result<TxStore> {
        validate_tx_id(tx_id)?;
        Ok(TxStore {
            store: Arc::clone(self),
            tx_id: Arc::from(tx_id),
        })
    }

    fn cf(&self, name: &'static str) -> Result<&ColumnFamily> {
        self.db
            .cf_handle(name)
            .ok_or(AgentTxError::MissingColumnFamily(name))
    }

    fn write(&self, batch: WriteBatch) -> Result<()> {
        let mut opts = WriteOptions::default();
        opts.set_sync(self.options.sync_writes);
        self.db.write_opt(batch, &opts)?;
        Ok(())
    }

    // ---------------------------------------------------------------------
    // Transaction registry
    // ---------------------------------------------------------------------

    /// Durably marks a transaction as open (used by crash recovery).
    pub fn register_transaction(&self, record: &TxRecord) -> Result<()> {
        validate_tx_id(&record.tx_id)?;
        self.db.put_cf(
            self.cf(CF_SNAPSHOTS)?,
            keys::tx_record(&record.tx_id),
            serde_json::to_vec(record)?,
        )?;
        Ok(())
    }

    /// Lists transactions that began but never committed or aborted.
    pub fn open_transactions(&self) -> Result<Vec<TxRecord>> {
        self.scan_prefix(CF_SNAPSHOTS, keys::TX_RECORD_PREFIX)?
            .into_iter()
            .map(|(_, v)| serde_json::from_slice(&v).map_err(Into::into))
            .collect()
    }

    // ---------------------------------------------------------------------
    // State access
    // ---------------------------------------------------------------------

    /// Reads a committed value (outside any transaction).
    pub fn get_committed(&self, key: &str) -> Result<Option<Vec<u8>>> {
        validate_key(key)?;
        let raw = self.db.get_cf(self.cf(CF_DEFAULT)?, keys::global(key))?;
        raw.map(|bytes| split_versioned(&bytes, key).map(|(_, v)| v.to_vec()))
            .transpose()
    }

    /// Reads `key` as seen by `tx`: its own uncommitted writes first, then
    /// committed state.
    pub fn get(&self, tx: &str, key: &str) -> Result<Option<Vec<u8>>> {
        validate_key(key)?;
        let data = self.cf(CF_DEFAULT)?;
        if let Some(raw) = self.db.get_cf(data, keys::overlay(tx, key))? {
            let (tag, _, value) = split_overlay(&raw, key)?;
            return Ok((tag == TAG_VALUE).then(|| value.to_vec()));
        }
        self.get_committed(key)
    }

    /// Writes `key` inside `tx` (journaled, invisible to others until commit).
    pub fn put(&self, tx: &str, key: &str, value: &[u8]) -> Result<()> {
        self.write_overlay(tx, key, Some(value))
    }

    /// Deletes `key` inside `tx` (journaled tombstone).
    pub fn delete(&self, tx: &str, key: &str) -> Result<()> {
        self.write_overlay(tx, key, None)
    }

    fn write_overlay(&self, tx: &str, key: &str, value: Option<&[u8]>) -> Result<()> {
        validate_key(key)?;
        let raw_key = keys::overlay(tx, key);
        let prior = self.db.get_cf(self.cf(CF_DEFAULT)?, &raw_key)?;
        // The base version is fixed by the first write of the key in this tx.
        let base_version = match &prior {
            Some(raw) => split_overlay(raw, key)?.1,
            None => self.committed_version(key)?,
        };
        let mut encoded = Vec::with_capacity(OVERLAY_HEADER_LEN + value.map_or(0, <[u8]>::len));
        encoded.push(if value.is_some() { TAG_VALUE } else { TAG_TOMBSTONE });
        encoded.extend_from_slice(&base_version.to_le_bytes());
        if let Some(v) = value {
            encoded.extend_from_slice(v);
        }
        self.journaled_write(tx, raw_key, prior, Some(encoded))
    }

    fn committed_version(&self, key: &str) -> Result<u64> {
        match self.db.get_cf(self.cf(CF_DEFAULT)?, keys::global(key))? {
            Some(raw) => Ok(split_versioned(&raw, key)?.0),
            None => Ok(0),
        }
    }

    /// Persists a step's output (journaled, discarded by rewinds past it).
    pub fn put_output(&self, tx: &str, step_id: u32, output: &[u8]) -> Result<()> {
        let raw_key = keys::output(tx, step_id);
        let prior = self.db.get_cf(self.cf(CF_DEFAULT)?, &raw_key)?;
        self.journaled_write(tx, raw_key, prior, Some(output.to_vec()))
    }

    pub fn get_output(&self, tx: &str, step_id: u32) -> Result<Option<Vec<u8>>> {
        Ok(self.db.get_cf(self.cf(CF_DEFAULT)?, keys::output(tx, step_id))?)
    }

    /// Atomically writes `new` to `raw_key` and records `prior` in the journal.
    fn journaled_write(
        &self,
        tx: &str,
        raw_key: Vec<u8>,
        prior: Option<Vec<u8>>,
        new: Option<Vec<u8>>,
    ) -> Result<()> {
        let seq = self.allocate_journal_seq(tx)?;
        let entry = JournalEntry {
            raw_key: raw_key.clone(),
            prior,
        };
        let mut batch = WriteBatch::default();
        batch.put_cf(self.cf(CF_SNAPSHOTS)?, keys::journal(tx, seq), entry.encode());
        let data = self.cf(CF_DEFAULT)?;
        match new {
            Some(value) => batch.put_cf(data, &raw_key, value),
            None => batch.delete_cf(data, &raw_key),
        }
        self.write(batch)
    }

    fn allocate_journal_seq(&self, tx: &str) -> Result<u64> {
        let mut next = self
            .journal_seq
            .entry(tx.to_owned())
            .or_try_insert_with(|| self.load_next_journal_seq(tx))?;
        let seq = *next;
        *next += 1;
        Ok(seq)
    }

    fn peek_journal_seq(&self, tx: &str) -> Result<u64> {
        let next = self
            .journal_seq
            .entry(tx.to_owned())
            .or_try_insert_with(|| self.load_next_journal_seq(tx))?;
        Ok(*next)
    }

    /// Recovers the next journal sequence number from disk (after restart).
    fn load_next_journal_seq(&self, tx: &str) -> Result<u64> {
        let prefix = keys::journal_prefix(tx);
        let upper = keys::prefix_end(&prefix);
        let mut iter = self
            .db
            .iterator_cf(self.cf(CF_SNAPSHOTS)?, IteratorMode::From(&upper, Direction::Reverse));
        match iter.next().transpose()? {
            Some((key, _)) if key.starts_with(&prefix) => keys::journal_seq_of(&key)
                .map(|seq| seq + 1)
                .ok_or_else(|| AgentTxError::Corrupt {
                    key: String::from_utf8_lossy(&key).into_owned(),
                    reason: "unparseable journal sequence".into(),
                }),
            _ => Ok(0),
        }
    }

    // ---------------------------------------------------------------------
    // Snapshots
    // ---------------------------------------------------------------------

    /// Captures the state of `tx` before `step_id` executes. O(1).
    ///
    /// Re-snapshotting the same step (after a rewind) overwrites the pointer.
    pub fn create_snapshot(&self, tx: &str, step_id: u32) -> Result<SnapshotId> {
        validate_tx_id(tx)?;
        let id = SnapshotId::new(tx, step_id);
        let meta = SnapshotMeta {
            id: id.clone(),
            journal_seq: self.peek_journal_seq(tx)?,
            rocksdb_sequence: self.db.latest_sequence_number(),
            created_at_ms: now_ms(),
        };
        self.db.put_cf(
            self.cf(CF_SNAPSHOTS)?,
            keys::snapshot(tx, step_id),
            serde_json::to_vec(&meta)?,
        )?;
        Ok(id)
    }

    pub fn snapshot_meta(&self, id: &SnapshotId) -> Result<Option<SnapshotMeta>> {
        validate_tx_id(&id.tx_id)?;
        self.db
            .get_cf(self.cf(CF_SNAPSHOTS)?, keys::snapshot(&id.tx_id, id.step_id))?
            .map(|bytes| serde_json::from_slice(&bytes).map_err(Into::into))
            .transpose()
    }

    /// Rewinds `id.tx_id` to exactly the state captured by `id`.
    ///
    /// Reverts every journaled mutation newer than the snapshot pointer, drops
    /// those journal entries and all later snapshots, in one atomic batch.
    /// Cost is O(mutations since snapshot); the snapshot itself stays valid.
    pub fn restore_snapshot(&self, id: &SnapshotId) -> Result<RestoreStats> {
        let started = Instant::now();
        let meta = self
            .snapshot_meta(id)?
            .ok_or_else(|| AgentTxError::SnapshotNotFound {
                tx_id: id.tx_id.clone(),
                step_id: id.step_id,
            })?;
        let tx = id.tx_id.as_str();
        let snaps = self.cf(CF_SNAPSHOTS)?;
        let data = self.cf(CF_DEFAULT)?;

        let prefix = keys::journal_prefix(tx);
        let lower = keys::journal(tx, meta.journal_seq);
        let upper = keys::prefix_end(&prefix);

        let mut batch = WriteBatch::default();
        let mut reverted = 0usize;
        // Newest first; within a WriteBatch the last operation on a key wins,
        // so the oldest prior value (the snapshot's) is what remains.
        for item in self
            .db
            .iterator_cf(snaps, IteratorMode::From(&upper, Direction::Reverse))
        {
            let (key, value) = item?;
            if !key.starts_with(&prefix) || key.as_ref() < lower.as_slice() {
                break;
            }
            let entry = JournalEntry::decode(&value)?;
            match entry.prior {
                Some(prior) => batch.put_cf(data, &entry.raw_key, prior),
                None => batch.delete_cf(data, &entry.raw_key),
            }
            reverted += 1;
        }
        batch.delete_range_cf(snaps, &lower, &upper);
        if let Some(next_step) = id.step_id.checked_add(1) {
            let later = keys::snapshot(tx, next_step);
            let snaps_end = keys::prefix_end(&keys::snapshot_prefix(tx));
            batch.delete_range_cf(snaps, &later, &snaps_end);
        }
        self.write(batch)?;
        self.journal_seq.insert(tx.to_owned(), meta.journal_seq);

        let stats = RestoreStats {
            entries_reverted: reverted,
            elapsed: started.elapsed(),
        };
        tracing::debug!(snapshot = %id, reverted, elapsed_us = stats.elapsed.as_micros() as u64, "snapshot restored");
        Ok(stats)
    }

    // ---------------------------------------------------------------------
    // Commit / discard
    // ---------------------------------------------------------------------

    /// Atomically publishes the overlay of `tx` and deletes all of its
    /// journal, snapshot, output and Saga records.
    ///
    /// Fails with [`AgentTxError::WriteConflict`] (writing nothing) if any key
    /// written by `tx` was committed by another transaction after `tx` first
    /// wrote it. Returns the number of keys published.
    pub fn commit(&self, tx: &str) -> Result<usize> {
        validate_tx_id(tx)?;
        let data = self.cf(CF_DEFAULT)?;
        let prefix = keys::overlay_prefix(tx);
        let _guard = self
            .commit_lock
            .lock()
            .map_err(|_| AgentTxError::Config("commit lock poisoned".into()))?;
        let version = self.commit_version.load(Ordering::SeqCst) + 1;

        let mut batch = WriteBatch::default();
        let mut published = 0usize;
        // Pinned point-in-time read view of the overlay.
        let view = self.db.snapshot();
        for item in view.iterator_cf(data, IteratorMode::From(&prefix, Direction::Forward)) {
            let (raw_key, raw_value) = item?;
            if !raw_key.starts_with(&prefix) {
                break;
            }
            let key = std::str::from_utf8(&raw_key[prefix.len()..]).map_err(|_| {
                AgentTxError::Corrupt {
                    key: String::from_utf8_lossy(&raw_key).into_owned(),
                    reason: "non-UTF-8 key".into(),
                }
            })?;
            let (tag, base_version, value) = split_overlay(&raw_value, key)?;
            if self.committed_version(key)? != base_version {
                return Err(AgentTxError::WriteConflict { key: key.to_owned() });
            }
            let global = keys::global(key);
            if tag == TAG_VALUE {
                let mut stamped = Vec::with_capacity(VERSION_LEN + value.len());
                stamped.extend_from_slice(&version.to_le_bytes());
                stamped.extend_from_slice(value);
                batch.put_cf(data, global, stamped);
            } else {
                batch.delete_cf(data, global);
            }
            published += 1;
        }
        drop(view);

        batch.put_cf(self.cf(CF_SNAPSHOTS)?, keys::COMMIT_VERSION, version.to_le_bytes());
        self.add_cleanup(&mut batch, tx)?;
        self.write(batch)?;
        self.commit_version.store(version, Ordering::SeqCst);
        self.journal_seq.remove(tx);
        Ok(published)
    }

    /// Deletes every trace of `tx` without publishing anything.
    pub fn discard(&self, tx: &str) -> Result<()> {
        validate_tx_id(tx)?;
        let mut batch = WriteBatch::default();
        self.add_cleanup(&mut batch, tx)?;
        self.write(batch)?;
        self.journal_seq.remove(tx);
        Ok(())
    }

    /// Deletes the overlay, journal and snapshots of `tx` but keeps its
    /// transaction record and Saga logs, so failed compensations remain
    /// visible to operators and are retried by crash recovery.
    pub fn discard_state(&self, tx: &str) -> Result<()> {
        validate_tx_id(tx)?;
        let mut batch = WriteBatch::default();
        self.add_state_cleanup(&mut batch, tx)?;
        self.write(batch)?;
        self.journal_seq.remove(tx);
        Ok(())
    }

    fn add_cleanup(&self, batch: &mut WriteBatch, tx: &str) -> Result<()> {
        self.add_state_cleanup(batch, tx)?;
        let saga_prefix = keys::saga_prefix(tx);
        let saga_end = keys::prefix_end(&saga_prefix);
        batch.delete_range_cf(self.cf(CF_SAGA_LOGS)?, saga_prefix, saga_end);
        batch.delete_cf(self.cf(CF_SNAPSHOTS)?, keys::tx_record(tx));
        Ok(())
    }

    fn add_state_cleanup(&self, batch: &mut WriteBatch, tx: &str) -> Result<()> {
        let data = self.cf(CF_DEFAULT)?;
        let snaps = self.cf(CF_SNAPSHOTS)?;
        for (cf, prefix) in [
            (data, keys::tx_data_prefix(tx)),
            (snaps, keys::journal_prefix(tx)),
            (snaps, keys::snapshot_prefix(tx)),
        ] {
            let end = keys::prefix_end(&prefix);
            batch.delete_range_cf(cf, prefix, end);
        }
        Ok(())
    }

    // ---------------------------------------------------------------------
    // Saga logs
    // ---------------------------------------------------------------------

    pub fn put_saga_record(&self, tx: &str, step_id: u32, seq: u64, record: &[u8]) -> Result<()> {
        validate_tx_id(tx)?;
        self.db
            .put_cf(self.cf(CF_SAGA_LOGS)?, keys::saga(tx, step_id, seq), record)?;
        Ok(())
    }

    pub fn delete_saga_record(&self, tx: &str, step_id: u32, seq: u64) -> Result<()> {
        validate_tx_id(tx)?;
        self.db
            .delete_cf(self.cf(CF_SAGA_LOGS)?, keys::saga(tx, step_id, seq))?;
        Ok(())
    }

    /// Persisted Saga records of `tx` in execution order.
    pub fn saga_records(&self, tx: &str) -> Result<Vec<Vec<u8>>> {
        validate_tx_id(tx)?;
        Ok(self
            .scan_prefix(CF_SAGA_LOGS, &keys::saga_prefix(tx))?
            .into_iter()
            .map(|(_, v)| v.into_vec())
            .collect())
    }

    fn scan_prefix(
        &self,
        cf: &'static str,
        prefix: &[u8],
    ) -> Result<Vec<RawEntry>> {
        let mut out = Vec::new();
        for item in self
            .db
            .iterator_cf(self.cf(cf)?, IteratorMode::From(prefix, Direction::Forward))
        {
            let (k, v) = item?;
            if !k.starts_with(prefix) {
                break;
            }
            out.push((k, v));
        }
        Ok(out)
    }
}

/// A [`RocksStore`] handle bound to a single transaction.
#[derive(Clone)]
pub struct TxStore {
    store: Arc<RocksStore>,
    tx_id: Arc<str>,
}

impl std::fmt::Debug for TxStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TxStore").field("tx_id", &self.tx_id).finish()
    }
}

impl TxStore {
    pub fn tx_id(&self) -> &str {
        &self.tx_id
    }

    pub fn inner(&self) -> &Arc<RocksStore> {
        &self.store
    }

    pub fn get(&self, key: &str) -> Result<Option<Vec<u8>>> {
        self.store.get(&self.tx_id, key)
    }

    pub fn put(&self, key: &str, value: &[u8]) -> Result<()> {
        self.store.put(&self.tx_id, key, value)
    }

    pub fn delete(&self, key: &str) -> Result<()> {
        self.store.delete(&self.tx_id, key)
    }

    pub fn put_output(&self, step_id: u32, output: &[u8]) -> Result<()> {
        self.store.put_output(&self.tx_id, step_id, output)
    }

    pub fn get_output(&self, step_id: u32) -> Result<Option<Vec<u8>>> {
        self.store.get_output(&self.tx_id, step_id)
    }

    /// Captures the transaction state before `step_id` runs.
    pub fn create_snapshot(&self, step_id: u32) -> Result<SnapshotId> {
        self.store.create_snapshot(&self.tx_id, step_id)
    }

    /// Rewinds this transaction to `snapshot_id`.
    pub fn restore_snapshot(&self, snapshot_id: &SnapshotId) -> Result<RestoreStats> {
        if snapshot_id.tx_id.as_str() != &*self.tx_id {
            return Err(AgentTxError::InvalidArgument(format!(
                "snapshot {snapshot_id} does not belong to transaction {}",
                self.tx_id
            )));
        }
        self.store.restore_snapshot(snapshot_id)
    }
}

fn validate_tx_id(tx_id: &str) -> Result<()> {
    let valid = !tx_id.is_empty()
        && tx_id.len() <= 128
        && tx_id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');
    if valid {
        Ok(())
    } else {
        Err(AgentTxError::InvalidArgument(format!(
            "invalid transaction id `{tx_id}`: expected 1-128 chars of [A-Za-z0-9_-]"
        )))
    }
}

fn validate_key(key: &str) -> Result<()> {
    if key.is_empty() || key.len() > 1024 {
        return Err(AgentTxError::InvalidArgument(
            "state keys must be 1-1024 bytes".into(),
        ));
    }
    Ok(())
}

fn decode_u64(bytes: &[u8], key: &str) -> Result<u64> {
    let arr: [u8; VERSION_LEN] = bytes.try_into().map_err(|_| AgentTxError::Corrupt {
        key: key.to_owned(),
        reason: "expected 8-byte integer".into(),
    })?;
    Ok(u64::from_le_bytes(arr))
}

/// Splits a committed value into `(version, payload)`.
fn split_versioned<'a>(raw: &'a [u8], key: &str) -> Result<(u64, &'a [u8])> {
    if raw.len() < VERSION_LEN {
        return Err(AgentTxError::Corrupt {
            key: key.to_owned(),
            reason: "committed value shorter than version header".into(),
        });
    }
    let (version, payload) = raw.split_at(VERSION_LEN);
    Ok((decode_u64(version, key)?, payload))
}

/// Splits an overlay value into `(tag, base_version, payload)`.
fn split_overlay<'a>(raw: &'a [u8], key: &str) -> Result<(u8, u64, &'a [u8])> {
    if raw.len() < OVERLAY_HEADER_LEN || raw[0] > TAG_VALUE {
        return Err(AgentTxError::Corrupt {
            key: key.to_owned(),
            reason: "malformed overlay header".into(),
        });
    }
    let base = decode_u64(&raw[1..OVERLAY_HEADER_LEN], key)?;
    Ok((raw[0], base, &raw[OVERLAY_HEADER_LEN..]))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn open() -> (tempfile::TempDir, Arc<RocksStore>) {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = RocksStore::open(dir.path(), StoreOptions::default()).expect("open");
        (dir, Arc::new(store))
    }

    fn get_str(store: &RocksStore, tx: &str, key: &str) -> Option<String> {
        store
            .get(tx, key)
            .unwrap()
            .map(|v| String::from_utf8(v).unwrap())
    }

    #[test]
    fn overlay_is_isolated_until_commit() {
        let (_dir, store) = open();
        store.put("t1", "k", b"v1").unwrap();
        assert_eq!(get_str(&store, "t1", "k").as_deref(), Some("v1"));
        assert_eq!(store.get("t2", "k").unwrap(), None);
        assert_eq!(store.get_committed("k").unwrap(), None);

        assert_eq!(store.commit("t1").unwrap(), 1);
        assert_eq!(store.get_committed("k").unwrap().as_deref(), Some(&b"v1"[..]));
        assert_eq!(get_str(&store, "t2", "k").as_deref(), Some("v1"));
    }

    #[test]
    fn restore_reverts_to_exact_snapshot_state() {
        let (_dir, store) = open();
        let tx = store.scoped("tx").unwrap();
        let s0 = tx.create_snapshot(0).unwrap();
        tx.put("a", b"1").unwrap();
        let s1 = tx.create_snapshot(1).unwrap();
        tx.put("a", b"2").unwrap();
        tx.put("b", b"x").unwrap();
        tx.put_output(1, b"{}").unwrap();
        tx.create_snapshot(2).unwrap();
        tx.delete("a").unwrap();

        let stats = tx.restore_snapshot(&s1).unwrap();
        assert_eq!(stats.entries_reverted, 4);
        assert_eq!(get_str(&store, "tx", "a").as_deref(), Some("1"));
        assert_eq!(store.get("tx", "b").unwrap(), None);
        assert_eq!(tx.get_output(1).unwrap(), None);
        assert!(store.snapshot_meta(&SnapshotId::new("tx", 2)).unwrap().is_none());
        assert!(store.snapshot_meta(&s1).unwrap().is_some());

        // Writes after a restore are journaled relative to the new pointer.
        tx.put("c", b"y").unwrap();
        tx.restore_snapshot(&s0).unwrap();
        assert_eq!(store.get("tx", "a").unwrap(), None);
        assert_eq!(store.get("tx", "c").unwrap(), None);
    }

    #[test]
    fn restore_is_repeatable_on_same_snapshot() {
        let (_dir, store) = open();
        let tx = store.scoped("tx").unwrap();
        tx.put("k", b"base").unwrap();
        let s = tx.create_snapshot(1).unwrap();
        for i in 0..3 {
            tx.put("k", format!("attempt-{i}").as_bytes()).unwrap();
            tx.restore_snapshot(&s).unwrap();
            assert_eq!(get_str(&store, "tx", "k").as_deref(), Some("base"));
        }
    }

    #[test]
    fn missing_snapshot_is_an_error() {
        let (_dir, store) = open();
        let err = store.restore_snapshot(&SnapshotId::new("tx", 9)).unwrap_err();
        assert!(matches!(err, AgentTxError::SnapshotNotFound { step_id: 9, .. }));
    }

    #[test]
    fn journal_sequence_survives_reopen() {
        let dir = tempfile::tempdir().unwrap();
        {
            let store = RocksStore::open(dir.path(), StoreOptions::default()).unwrap();
            store.put("tx", "a", b"1").unwrap();
            store.create_snapshot("tx", 1).unwrap();
            store.put("tx", "a", b"2").unwrap();
        }
        let store = RocksStore::open(dir.path(), StoreOptions::default()).unwrap();
        store.put("tx", "a", b"3").unwrap();
        store.restore_snapshot(&SnapshotId::new("tx", 1)).unwrap();
        assert_eq!(get_str(&store, "tx", "a").as_deref(), Some("1"));
    }

    #[test]
    fn first_committer_wins_on_write_conflict() {
        let (_dir, store) = open();
        store.put("a", "shared", b"from-a").unwrap();
        store.put("b", "shared", b"from-b").unwrap();
        store.commit("a").unwrap();
        let err = store.commit("b").unwrap_err();
        assert!(matches!(err, AgentTxError::WriteConflict { ref key } if key == "shared"));
        assert_eq!(store.get_committed("shared").unwrap().as_deref(), Some(&b"from-a"[..]));

        // A transaction starting after the commit sees and may overwrite it.
        store.put("c", "shared", b"from-c").unwrap();
        store.commit("c").unwrap();
        assert_eq!(store.get_committed("shared").unwrap().as_deref(), Some(&b"from-c"[..]));
    }

    #[test]
    fn tombstones_delete_committed_keys() {
        let (_dir, store) = open();
        store.put("a", "k", b"v").unwrap();
        store.commit("a").unwrap();
        store.delete("b", "k").unwrap();
        assert_eq!(store.get("b", "k").unwrap(), None);
        store.commit("b").unwrap();
        assert_eq!(store.get_committed("k").unwrap(), None);
    }

    #[test]
    fn discard_and_commit_remove_all_tx_records() {
        let (_dir, store) = open();
        store
            .register_transaction(&TxRecord {
                tx_id: "tx".into(),
                agent_id: "agent".into(),
                created_at_ms: 1,
            })
            .unwrap();
        store.create_snapshot("tx", 0).unwrap();
        store.put("tx", "k", b"v").unwrap();
        store.put_saga_record("tx", 1, 0, b"{}").unwrap();
        assert_eq!(store.open_transactions().unwrap().len(), 1);

        store.discard_state("tx").unwrap();
        assert_eq!(store.get("tx", "k").unwrap(), None);
        assert_eq!(store.open_transactions().unwrap().len(), 1, "record kept");
        assert_eq!(store.saga_records("tx").unwrap().len(), 1, "saga logs kept");

        store.discard("tx").unwrap();
        assert!(store.open_transactions().unwrap().is_empty());
        assert!(store.saga_records("tx").unwrap().is_empty());
        assert!(store.snapshot_meta(&SnapshotId::new("tx", 0)).unwrap().is_none());
        assert_eq!(store.get("tx", "k").unwrap(), None);
    }

    #[test]
    fn rejects_invalid_identifiers() {
        let (_dir, store) = open();
        assert!(store.scoped("bad/id").is_err());
        assert!(store.put("tx", "", b"v").is_err());
    }
}
