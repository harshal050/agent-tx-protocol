//! Durable state, snapshot and Saga-log storage on embedded RocksDB.
//!
//! # Layout
//!
//! | Column family | Key                              | Value                              |
//! |---------------|----------------------------------|------------------------------------|
//! | `default`     | `g/{key}`                        | committed value (`version ‖ bytes`)|
//! | `default`     | `x/{tx}/s/{key}`                 | tx overlay (`tag ‖ base ‖ bytes`)  |
//! | `default`     | `x/{tx}/o/{step}`                | step output JSON                   |
//! | `snapshots`   | `t/{tx}`                         | open transaction record            |
//! | `snapshots`   | `m/{tx}/{step}`                  | [`SnapshotMeta`]                   |
//! | `snapshots`   | `j/{tx}/{seq}`                   | [`JournalEntry`] (undo journal)    |
//! | `snapshots`   | `c/commit_version`               | global commit version counter      |
//! | `saga_logs`   | `u/{tx}/{step}/{seq}`            | persisted Saga undo record         |

pub mod rocks;
pub mod snapshot;

pub use rocks::{
    CF_DEFAULT, CF_SAGA_LOGS, CF_SNAPSHOTS, RocksStore, StoreOptions, TxRecord, TxStore,
};
pub use snapshot::{JournalEntry, RestoreStats, SnapshotId, SnapshotMeta};
