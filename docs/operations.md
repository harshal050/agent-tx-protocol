---
title: Deploy & operate
description: Run AgentTx in production — Docker, recommended settings, health checks, logs, backups and recovery.
---

## Docker

```bash
docker build -t agenttx .
docker run -d --name agenttx \
  -p 50051:50051 \
  -v agenttx-data:/data \
  -e AGENTTX_SYNC_WRITES=true \
  -e AGENTTX_LOG_FORMAT=json \
  agenttx
```

The image runs as a non-root user. It keeps RocksDB in `/data/rocksdb`, the sandbox in `/data/sandbox` and the outbox in `/data/outbox.jsonl`.

## Recommended production settings

| Setting | Value | Reason |
|---|---|---|
| `AGENTTX_LISTEN_ADDR` | `0.0.0.0:50051` behind a private network or mTLS proxy | AgentTx does not terminate TLS itself |
| `AGENTTX_SYNC_WRITES` | `true` | survive power loss |
| `AGENTTX_LOG_FORMAT` | `json` | structured logs for your pipeline |
| `AGENTTX_STEP_TIMEOUT_MS` | slightly above your slowest tool | timeouts count as step failures |
| `AGENTTX_TX_IDLE_TIMEOUT_SECS` | 2–5 × your longest agent think time | reclaims abandoned transactions |
| `RUST_LOG` | `info` (or `agenttx=debug`) | log verbosity |

## Security

- Put AgentTx behind a service mesh, an Envoy sidecar or a private network. Every client that can reach the port can execute tools.
- The built-in `fs.*` tools are confined to `--fs-root`. Absolute paths, `..` and symlinks that escape the root are rejected.
- Request sizes are capped: arguments and context at 1 MiB each, messages at 4 MiB.

## Health checks

AgentTx implements the standard `grpc.health.v1.Health` service:

```bash
grpc_health_probe -addr=127.0.0.1:50051 -service=agenttx.v1.AgentTxService
```

Kubernetes (1.24+) can use a native gRPC probe:

```yaml
livenessProbe:
  grpc:
    port: 50051
    service: agenttx.v1.AgentTxService
```

## Logs worth alerting on

| Message | Meaning |
|---|---|
| `compensation failed` | an undo action exhausted its retries; manual follow-up needed |
| `transaction aborted with failed compensations; saga logs retained` | the same, at transaction level |
| `committed effects failed to dispatch` | the outbox write failed after commit |
| `recovery incomplete; saga logs retained` | startup recovery couldn't compensate something |

## Outbox relay

The outbox is append-only JSON Lines. Run a relay that tails the file, delivers each effect using its `idempotency_key`, and records its offset. Rotate the file only after the relay has caught up.

## Backups

RocksDB files can't be copied safely while the process is writing. Either stop the process and snapshot the volume, or use filesystem or volume snapshots, which are crash-consistent thanks to the WAL. Open transactions in a restored backup are compensated on start.

## Graceful shutdown

On `SIGTERM` or Ctrl-C the server stops accepting new calls and drains in-flight requests. Transactions that are still open stay on disk and are compensated by recovery on the next start. Commit or roll back your tasks before planned restarts.
