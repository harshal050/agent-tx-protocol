//! Execution ledger: step dependency DAG and Saga compensation ledger.

pub mod dependency_graph;
pub mod saga;

pub use dependency_graph::{Dependency, DependencyGraph, DependencyKind};
pub use saga::{
    CompensationReport, EffectDispatcher, FlushReport, FnUndo, OutboxDispatcher, SagaLedger,
    SagaRecord, StagedEffect, StagingQueue, UndoAction, UndoRegistry,
};
