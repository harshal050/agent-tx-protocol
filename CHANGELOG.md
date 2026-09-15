# Changelog

All notable changes to this project are documented here. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project uses [Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added

- **Protocol server** (`crates/agenttx-protocol`): tonic gRPC service `agenttx.v1.AgentTxService` with health checks and graceful shutdown.
- **Transactional storage** on RocksDB: per-transaction overlays, undo-journal snapshots with O(1) capture and change-proportional restore, atomic commit with first-committer-wins conflict detection.
- **Hybrid rollback controller**: dependency jump, local backtrack, global Step-0 reset and abort, bounded by an O(N log N) replay budget.
- **Dependency graph** built on `petgraph`, with explicit `${steps.N.path}` references and implicit value linking for root-cause search.
- **Saga ledger** with crash-recoverable undo actions, a staging queue for non-reversible effects and a JSONL outbox dispatcher.
- **Deterministic error cleaner** with about 30 rules and a salient-line fallback that produces one-line Clean Hints.
- Built-in tools: `kv.*`, `record.*`, `fs.*` (sandboxed) and `effect.stage`.
- **Benchmark harness** (`crates/agenttx-bench`): before/after agent recovery simulation, per-step latency, restore and commit scaling, error cleaning and throughput.
- **Documentation website** (`apps/docs`, Next.js): docs and benchmark results loaded from GitHub with a local fallback, ⌘K search, interactive benchmark charts and webhook revalidation.
- Turborepo monorepo, CI workflows, Dockerfile and community health files.

### Fixed

- Commit, discard and restore use point deletes instead of `delete_range`, and every prefix scan uses explicit iterator bounds. Accumulated range tombstones had been slowing all later reads; bounded scans never visit other transactions' deleted keys (roughly 3× single-transaction throughput in `agenttx-bench`).

[Unreleased]: https://github.com/harshal050/agent-tx-protocol/commits/main
