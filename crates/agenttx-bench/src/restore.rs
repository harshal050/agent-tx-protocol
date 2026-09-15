//! Snapshot restore and commit scaling.

use std::sync::Arc;
use std::time::Instant;

use anyhow::ensure;

use agenttx::storage::{RocksStore, StoreOptions};

use crate::report::{CommitPoint, RestorePoint, RunConfig};
use crate::stats::{micros, summarize};

const VALUE_BYTES: usize = 128;
/// Distinct keys the journaled writes cycle over.
const KEY_SPACE: u32 = 1_000;

pub fn run_restore(config: &RunConfig) -> anyhow::Result<Vec<RestorePoint>> {
    let sizes: &[u32] = if config.quick {
        &[10, 100, 1_000, 10_000]
    } else {
        &[10, 100, 1_000, 10_000, 100_000]
    };
    let runs = if config.quick { 3 } else { 7 };
    let db = tempfile::tempdir()?;
    let store = Arc::new(RocksStore::open(db.path(), StoreOptions::default())?);
    let value = vec![0x5a_u8; VALUE_BYTES];

    let mut points = Vec::new();
    for &entries in sizes {
        let mut restore = Vec::with_capacity(runs);
        let mut forward = Vec::with_capacity(runs);
        for run in 0..runs {
            let tx = store.scoped(&format!("restore-{entries}-{run}"))?;
            let snapshot = tx.create_snapshot(1)?;

            let started = Instant::now();
            for i in 0..entries {
                tx.put(&format!("key-{}", i % KEY_SPACE), &value)?;
            }
            forward.push(micros(started.elapsed()));

            let started = Instant::now();
            let stats = tx.restore_snapshot(&snapshot)?;
            restore.push(micros(started.elapsed()));
            ensure!(
                stats.entries_reverted == entries as usize,
                "restore reverted {} of {entries}",
                stats.entries_reverted
            );
            store.discard(tx.tx_id())?;
        }
        points.push(RestorePoint {
            journal_entries: entries,
            restore: summarize(&mut restore),
            forward_writes: summarize(&mut forward),
        });
    }
    Ok(points)
}

pub fn run_commit(config: &RunConfig) -> anyhow::Result<Vec<CommitPoint>> {
    let sizes: &[u32] = if config.quick {
        &[10, 100, 1_000, 10_000]
    } else {
        &[10, 100, 1_000, 10_000, 50_000]
    };
    let runs = if config.quick { 3 } else { 5 };
    let db = tempfile::tempdir()?;
    let store = Arc::new(RocksStore::open(db.path(), StoreOptions::default())?);
    let value = vec![0x33_u8; VALUE_BYTES];

    let mut points = Vec::new();
    for &keys in sizes {
        let mut commit = Vec::with_capacity(runs);
        for run in 0..runs {
            let tx = store.scoped(&format!("commit-{keys}-{run}"))?;
            for i in 0..keys {
                tx.put(&format!("commit/{keys}/{run}/{i}"), &value)?;
            }
            let started = Instant::now();
            let published = store.commit(tx.tx_id())?;
            commit.push(micros(started.elapsed()));
            ensure!(
                published == keys as usize,
                "published {published} of {keys}"
            );
        }
        points.push(CommitPoint {
            keys,
            commit: summarize(&mut commit),
        });
    }
    Ok(points)
}
