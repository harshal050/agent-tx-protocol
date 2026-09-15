---
title: Custom tools
description: Implement the Tool trait, use transactional state, declare side effects correctly and register crash-recoverable undo actions.
---

Built-in tools cover state, records, files and staged effects. Real deployments add their own tools by implementing `Tool` and embedding the engine.

## The Tool trait

```rust
use agenttx::engine::{Tool, ToolContext, ToolError};
use async_trait::async_trait;
use serde_json::{json, Value};

pub struct CreateTicket {
    pub client: TicketClient,
}

#[async_trait]
impl Tool for CreateTicket {
    fn name(&self) -> &str {
        "tickets.create"
    }

    async fn execute(&self, ctx: &ToolContext, args: Value) -> Result<Value, ToolError> {
        let title = args["title"]
            .as_str()
            .ok_or_else(|| ToolError::failed("missing field `title`"))?;

        let ticket = self
            .client
            .create(title)
            .await
            .map_err(|e| ToolError::failed(e.to_string()))?;

        // Compensation: close the ticket if the transaction rewinds past this step.
        ctx.register_undo(Arc::new(CloseTicket { id: ticket.id.clone(), client: self.client.clone() }));

        Ok(json!({ "ticket_id": ticket.id, "url": ticket.url }))
    }
}
```

## Return errors the cleaner understands

`ToolError::Failed(message)` goes through the error cleaner, so pass the backend's original error text instead of a generic message. Formats that match [Clean Hint rules](./clean-hints.md) — `missing field `x``, PostgreSQL detail lines, HTTP status codes — produce precise hints and root-cause jumps.

Use `ToolError::Internal` only for infrastructure problems that the agent can't fix.

## State through the context

`ctx.store()` is the transaction's journaled overlay. Anything written there is rewound automatically and published atomically on commit:

```rust
ctx.store().put("tickets/last", serde_json::to_vec(&ticket)?.as_slice())?;
```

## Side effects

| Effect | Call | Rule |
|---|---|---|
| Reversible | `ctx.register_undo(action)` | register **before** performing the effect |
| Non-reversible | `ctx.stage_effect(channel, payload, key)` | never send directly from a tool |

## Crash-recoverable undo actions

```rust
#[derive(Debug)]
struct CloseTicket { id: String, client: TicketClient }

#[async_trait]
impl UndoAction for CloseTicket {
    fn kind(&self) -> &str { "tickets.close" }
    fn describe(&self) -> String { format!("close ticket {}", self.id) }
    fn payload(&self) -> Value { json!({ "id": self.id }) }
    async fn undo(&self) -> Result<(), String> {
        self.client.close(&self.id).await.map_err(|e| e.to_string())
    }
}
```

Register a factory so recovery can rebuild the action after a restart:

```rust
let client = ticket_client.clone();
undo_registry.register("tickets.close", move |payload| {
    let id = payload["id"].as_str().unwrap_or_default().to_owned();
    Ok(Arc::new(CloseTicket { id, client: client.clone() }) as Arc<dyn UndoAction>)
});
```

## Wiring it up

```rust
let tools = ToolRegistry::new();
builtin_tools::register_builtin_tools(&tools, "data/sandbox");
tools.register(Arc::new(CreateTicket { client: ticket_client.clone() }));

let undo = UndoRegistry::new();
builtin_tools::register_builtin_undo_factories(&undo);

let engine = Arc::new(Engine::new(store, tools, undo, dispatcher, config.engine_config()));
engine.recover().await?;
agenttx::grpc::serve(engine, listener, shutdown).await?;
```

## Checklist

- [ ] Undo actions are idempotent: running one twice has the same result as running it once.
- [ ] Undo is registered before the external call.
- [ ] Emails, webhooks and notifications are staged, not sent.
- [ ] Error messages keep the backend's original wording.
- [ ] Tools finish well within `--step-timeout-ms`.
