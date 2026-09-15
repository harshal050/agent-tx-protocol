---
title: Client integration
description: Connect an agent loop to AgentTx from Python, TypeScript or Rust, and handle rollbacks correctly.
---

Any gRPC client works. The contract is `crates/agenttx-protocol/proto/agenttx/v1/agenttx.proto`.

## The loop every client implements

```text
tx   = BeginTransaction(agent_id)
next = 1
while not done:
    call = agent.plan(next, constraints)            # your LLM decides the tool call
    r    = ExecuteStep(tx, next, call.tool, call.arguments_json, call.reasoning)
    if r.status == SUCCESS:
        agent.remember(next, r.output_json)
        next = r.next_step_id
    elif r.status == ROLLBACK_TRIGGERED:
        if r.context_reset: agent.clear_context()
        agent.forget_steps_from(r.rollback_to_step)
        constraints = r.constraints                  # add to the system prompt
        next = r.next_step_id
    else:  # FAILED
        raise TaskFailed(r.abort_reason)
CommitTransaction(tx)
```

Two rules matter most:

1. **Always send `next_step_id`.** Never keep your own counter after a rollback.
2. **Forget rewound steps.** Their outputs no longer exist, so the agent must re-plan them.

## Python

Generate stubs with `grpcio-tools`:

```bash
pip install grpcio grpcio-tools
python -m grpc_tools.protoc -I crates/agenttx-protocol/proto \
  --python_out=. --grpc_python_out=. agenttx/v1/agenttx.proto
```

```python
import json
import grpc
from agenttx.v1 import agenttx_pb2 as pb
from agenttx.v1 import agenttx_pb2_grpc as rpc

client = rpc.AgentTxServiceStub(grpc.insecure_channel("127.0.0.1:50051"))
tx = client.BeginTransaction(pb.BeginTransactionRequest(agent_id="billing-agent")).transaction_id

next_step, constraints = 1, []
for call in agent.run(constraints):  # your agent yields tool calls
    r = client.ExecuteStep(pb.ExecuteStepRequest(
        transaction_id=tx,
        step_id=next_step,
        tool_name=call.tool,
        arguments_json=json.dumps(call.arguments),
        raw_context=call.reasoning,
    ))
    if r.status == pb.STEP_STATUS_SUCCESS:
        agent.observe(json.loads(r.output_json))
    elif r.status == pb.STEP_STATUS_ROLLBACK_TRIGGERED:
        constraints = list(r.constraints)
        agent.rewind(r.rollback_to_step, reset_context=r.context_reset)
    else:
        raise RuntimeError(r.abort_reason)
    next_step = r.next_step_id

client.CommitTransaction(pb.CommitTransactionRequest(transaction_id=tx))
```

## TypeScript (Node.js)

```bash
pnpm add @grpc/grpc-js @grpc/proto-loader
```

```ts
import * as grpc from "@grpc/grpc-js";
import * as protoLoader from "@grpc/proto-loader";
import { promisify } from "node:util";

const definition = protoLoader.loadSync("crates/agenttx-protocol/proto/agenttx/v1/agenttx.proto", {
  keepCase: true,
  enums: String,
  defaults: true,
});
const { agenttx } = grpc.loadPackageDefinition(definition) as any;
const client = new agenttx.v1.AgentTxService("127.0.0.1:50051", grpc.credentials.createInsecure());

const begin = promisify(client.BeginTransaction.bind(client));
const execute = promisify(client.ExecuteStep.bind(client));
const commit = promisify(client.CommitTransaction.bind(client));

const { transaction_id } = await begin({ agent_id: "support-agent" });
const r = await execute({
  transaction_id,
  step_id: 1,
  tool_name: "kv.put",
  arguments_json: JSON.stringify({ key: "ticket", value: "T-81" }),
  raw_context: "Store the ticket id for later steps.",
});
if (r.status === "STEP_STATUS_ROLLBACK_TRIGGERED") console.log(r.clean_hint, r.next_step_id);
await commit({ transaction_id });
```

## Rust

Embed the engine directly, with no network hop:

```rust
use agenttx::engine::{StepRequest, StepStatus};

let tx = engine.begin("agent", Default::default(), None).await?.tx_id;
let outcome = engine.execute_step(StepRequest {
    tx_id: tx.clone(),
    step_id: 1,
    tool_name: "kv.put".into(),
    arguments_json: r#"{"key":"k","value":1}"#.into(),
    raw_context: String::new(),
}).await?;
assert_eq!(outcome.status, StepStatus::Success);
engine.commit(&tx).await?;
```

Or use the generated tonic client: `agenttx::proto::agent_tx_service_client::AgentTxServiceClient`.

## Error handling

| Situation | What you get | What to do |
|---|---|---|
| Tool failed | `ROLLBACK_TRIGGERED` + hint | re-plan from `next_step_id` |
| Controller gave up | `FAILED` + `abort_reason` | surface to the user; everything is already compensated |
| `FAILED_PRECONDITION` | wrong step id or ended transaction | resync with `GetTransaction` |
| `ABORTED` on commit | write conflict | start a new transaction |
| `UNAVAILABLE` / `INTERNAL` | infrastructure | retry the same step id with backoff |
