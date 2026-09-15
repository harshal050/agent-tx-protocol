---
title: Quickstart
description: Start the AgentTx server, run a transaction with grpcurl, and watch a dependency jump rollback happen.
---

This guide takes about five minutes. You need Rust 1.88+, a C++ toolchain with libclang (RocksDB is built from source; see [Installation](./installation.md)), and [`grpcurl`](https://github.com/fullstorydev/grpcurl).

## 1. Start the server

```bash
git clone https://github.com/harshal050/agent-tx-protocol.git
cd agent-tx-protocol
cargo run --release -p agenttx-protocol -- --listen-addr 127.0.0.1:50051
```

The first build compiles RocksDB and takes a few minutes. When it's ready you'll see:

```text
INFO AgentTx gRPC server listening addr=127.0.0.1:50051
```

## 2. Begin a transaction

In a second terminal:

```bash
export P="-plaintext -import-path crates/agenttx-protocol/proto -proto agenttx/v1/agenttx.proto"
TX=$(grpcurl $P -d '{"agent_id":"quickstart"}' \
  127.0.0.1:50051 agenttx.v1.AgentTxService/BeginTransaction | jq -r .transactionId)
echo $TX
```

## 3. Run a step that stores bad data

Step 1 picks a customer that doesn't exist. Nothing fails yet.

```bash
grpcurl $P -d @ 127.0.0.1:50051 agenttx.v1.AgentTxService/ExecuteStep <<EOF
{
  "transaction_id": "$TX",
  "step_id": 1,
  "tool_name": "kv.put",
  "arguments_json": "{\"key\":\"customer\",\"value\":\"c-404\"}"
}
EOF
```

## 4. Hit the failure later

Step 2 references step 1's output with `${steps.1.value}` and tries to create an invoice for that customer:

```bash
grpcurl $P -d @ 127.0.0.1:50051 agenttx.v1.AgentTxService/ExecuteStep <<EOF
{
  "transaction_id": "$TX",
  "step_id": 2,
  "tool_name": "record.insert",
  "arguments_json": "{\"table\":\"invoices\",\"id\":\"inv-1\",\"fields\":{\"customer_id\":\"\${steps.1.value}\"},\"references\":{\"customer_id\":\"customers\"}}"
}
EOF
```

The response:

```json
{
  "status": "STEP_STATUS_ROLLBACK_TRIGGERED",
  "cleanHint": "Hint: Foreign key constraint failed for 'customer_id'. Ensure target customer exists before step execution.",
  "strategy": "ROLLBACK_STRATEGY_DEPENDENCY_JUMP",
  "rollbackToStep": 1,
  "nextStepId": 1,
  "constraints": ["Hint: Foreign key constraint failed for 'customer_id'. Ensure target customer exists before step execution."],
  "errorCategory": "foreign_key_violation"
}
```

AgentTx traced `customer_id` back to step 1, rewound both steps and asked the agent to resume at step 1.

## 5. Fix it and commit

Resume at step 1 with a customer that exists, then commit:

```bash
grpcurl $P -d "{\"transaction_id\":\"$TX\",\"step_id\":1,\"tool_name\":\"record.insert\",\"arguments_json\":\"{\\\"table\\\":\\\"customers\\\",\\\"id\\\":\\\"c-1\\\"}\"}" \
  127.0.0.1:50051 agenttx.v1.AgentTxService/ExecuteStep

grpcurl $P -d "{\"transaction_id\":\"$TX\"}" 127.0.0.1:50051 agenttx.v1.AgentTxService/CommitTransaction
```

## Next steps

- Connect a real agent: [Client integration](./client-integration.md).
- Learn how the resume step is chosen: [Rollback strategies](./rollback-strategies.md).
- Add your own tools: [Custom tools](./custom-tools.md).
