//! Snapshot identifiers, metadata, the binary undo-journal format and key layout.
//!
//! RocksDB's native `db.snapshot()` only pins a *read view* at a sequence
//! number; it cannot rewind the database. AgentTx therefore implements
//! rewinds with a per-transaction undo journal:
//!
//! * every mutation writes, in the same atomic `WriteBatch`, a
//!   [`JournalEntry`] holding the key's prior value;
//! * a snapshot is just a pointer (`journal_seq`) into that journal;
//! * restoring replays journal entries newer than the pointer in reverse into
//!   one `WriteBatch`, so cost is proportional to the number of mutations since
//!   the snapshot — never to database size.

use std::fmt;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::errors::{AgentTxError, Result};

/// Identifies the state of a transaction *immediately before* `step_id` ran.
///
/// Step 0 is the initial state captured by `BeginTransaction`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SnapshotId {
    pub tx_id: String,
    pub step_id: u32,
}

impl SnapshotId {
    pub fn new(tx_id: impl Into<String>, step_id: u32) -> Self {
        Self {
            tx_id: tx_id.into(),
            step_id,
        }
    }

    /// `true` for the Step-0 snapshot taken at transaction begin.
    pub fn is_initial(&self) -> bool {
        self.step_id == 0
    }
}

impl fmt::Display for SnapshotId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}@{}", self.tx_id, self.step_id)
    }
}

/// Persisted snapshot metadata.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnapshotMeta {
    pub id: SnapshotId,
    /// First journal sequence number written *after* this snapshot.
    pub journal_seq: u64,
    /// RocksDB's latest sequence number when the snapshot was taken (audit).
    pub rocksdb_sequence: u64,
    pub created_at_ms: u64,
}

/// One undo-journal record: the value `raw_key` held before a mutation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JournalEntry {
    /// Full key in the `default` column family.
    pub raw_key: Vec<u8>,
    /// Prior raw value; `None` if the key did not exist.
    pub prior: Option<Vec<u8>>,
}

impl JournalEntry {
    const TAG_ABSENT: u8 = 0;
    const TAG_PRESENT: u8 = 1;

    /// Encodes as `key_len: u32 LE ‖ key ‖ tag: u8 ‖ prior`.
    pub fn encode(&self) -> Vec<u8> {
        let prior_len = self.prior.as_ref().map_or(0, Vec::len);
        let mut out = Vec::with_capacity(4 + self.raw_key.len() + 1 + prior_len);
        out.extend_from_slice(&(self.raw_key.len() as u32).to_le_bytes());
        out.extend_from_slice(&self.raw_key);
        match &self.prior {
            Some(prior) => {
                out.push(Self::TAG_PRESENT);
                out.extend_from_slice(prior);
            }
            None => out.push(Self::TAG_ABSENT),
        }
        out
    }

    pub fn decode(bytes: &[u8]) -> Result<Self> {
        let corrupt = |reason: &str| AgentTxError::Corrupt {
            key: "journal entry".into(),
            reason: reason.into(),
        };
        let (len_bytes, rest) = bytes
            .split_first_chunk::<4>()
            .ok_or_else(|| corrupt("truncated length header"))?;
        let key_len = u32::from_le_bytes(*len_bytes) as usize;
        if rest.len() < key_len + 1 {
            return Err(corrupt("truncated key"));
        }
        let (raw_key, rest) = rest.split_at(key_len);
        let (tag, body) = rest.split_first().ok_or_else(|| corrupt("missing tag"))?;
        let prior = match *tag {
            Self::TAG_ABSENT if body.is_empty() => None,
            Self::TAG_ABSENT => return Err(corrupt("absent tag with trailing bytes")),
            Self::TAG_PRESENT => Some(body.to_vec()),
            _ => return Err(corrupt("unknown tag")),
        };
        Ok(Self {
            raw_key: raw_key.to_vec(),
            prior,
        })
    }
}

/// Measurements from a snapshot restore.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RestoreStats {
    pub entries_reverted: usize,
    pub elapsed: Duration,
}

pub(crate) fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

/// Key construction helpers. All numeric components are zero-padded so that
/// lexicographic order equals numeric order.
pub(crate) mod keys {
    pub const TX_RECORD_PREFIX: &[u8] = b"t/";
    pub const COMMIT_VERSION: &[u8] = b"c/commit_version";

    pub fn global(key: &str) -> Vec<u8> {
        format!("g/{key}").into_bytes()
    }

    pub fn tx_data_prefix(tx: &str) -> Vec<u8> {
        format!("x/{tx}/").into_bytes()
    }

    pub fn overlay_prefix(tx: &str) -> Vec<u8> {
        format!("x/{tx}/s/").into_bytes()
    }

    pub fn overlay(tx: &str, key: &str) -> Vec<u8> {
        format!("x/{tx}/s/{key}").into_bytes()
    }

    pub fn output(tx: &str, step: u32) -> Vec<u8> {
        format!("x/{tx}/o/{step:010}").into_bytes()
    }

    pub fn tx_record(tx: &str) -> Vec<u8> {
        format!("t/{tx}").into_bytes()
    }

    pub fn snapshot_prefix(tx: &str) -> Vec<u8> {
        format!("m/{tx}/").into_bytes()
    }

    pub fn snapshot(tx: &str, step: u32) -> Vec<u8> {
        format!("m/{tx}/{step:010}").into_bytes()
    }

    pub fn journal_prefix(tx: &str) -> Vec<u8> {
        format!("j/{tx}/").into_bytes()
    }

    pub fn journal(tx: &str, seq: u64) -> Vec<u8> {
        format!("j/{tx}/{seq:020}").into_bytes()
    }

    pub fn saga_prefix(tx: &str) -> Vec<u8> {
        format!("u/{tx}/").into_bytes()
    }

    pub fn saga(tx: &str, step: u32, seq: u64) -> Vec<u8> {
        format!("u/{tx}/{step:010}/{seq:020}").into_bytes()
    }

    /// Parses the trailing zero-padded sequence number of a journal key.
    pub fn journal_seq_of(key: &[u8]) -> Option<u64> {
        let tail = key.rsplit(|b| *b == b'/').next()?;
        std::str::from_utf8(tail).ok()?.parse().ok()
    }

    /// Smallest key strictly greater than every key starting with `prefix`.
    pub fn prefix_end(prefix: &[u8]) -> Vec<u8> {
        let mut end = prefix.to_vec();
        while let Some(last) = end.pop() {
            if last < u8::MAX {
                end.push(last + 1);
                return end;
            }
        }
        // Prefix was all 0xFF bytes: no finite upper bound; use a max key.
        vec![u8::MAX; prefix.len() + 1]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn journal_entry_roundtrip() {
        for entry in [
            JournalEntry {
                raw_key: b"x/t/s/a".to_vec(),
                prior: Some(b"hello".to_vec()),
            },
            JournalEntry {
                raw_key: b"x/t/s/b".to_vec(),
                prior: None,
            },
            JournalEntry {
                raw_key: Vec::new(),
                prior: Some(Vec::new()),
            },
        ] {
            assert_eq!(JournalEntry::decode(&entry.encode()).unwrap(), entry);
        }
    }

    #[test]
    fn journal_entry_rejects_corruption() {
        assert!(JournalEntry::decode(&[1, 0]).is_err());
        assert!(JournalEntry::decode(&[5, 0, 0, 0, b'a']).is_err());
        assert!(JournalEntry::decode(&[0, 0, 0, 0, 9]).is_err());
        assert!(JournalEntry::decode(&[0, 0, 0, 0, 0, 1]).is_err());
    }

    #[test]
    fn keys_sort_numerically() {
        assert!(keys::journal("t", 9) < keys::journal("t", 10));
        assert!(keys::snapshot("t", 2) < keys::snapshot("t", 11));
        assert_eq!(keys::journal_seq_of(&keys::journal("t", 42)), Some(42));
    }

    #[test]
    fn prefix_end_bounds_prefix() {
        let prefix = keys::journal_prefix("tx1");
        let end = keys::prefix_end(&prefix);
        assert!(keys::journal("tx1", u64::MAX) < end);
        assert!(keys::journal_prefix("tx10") >= end || !keys::journal_prefix("tx10").starts_with(&prefix));
        assert_eq!(keys::prefix_end(&[0xFF]), vec![0xFF, 0xFF]);
    }

    #[test]
    fn snapshot_id_display() {
        let id = SnapshotId::new("abc", 3);
        assert_eq!(id.to_string(), "abc@3");
        assert!(!id.is_initial());
        assert!(SnapshotId::new("abc", 0).is_initial());
    }
}
