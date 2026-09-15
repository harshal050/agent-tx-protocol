//! MCP tools exposed to AI agents, and their handlers.
//!
//! The tool texts are written for the model: every response says what
//! happened and exactly what to call next.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use serde_json::{Value, json};

use crate::engine::{Engine, StepRequest, StepStatus};

/// State shared by all tool handlers.
pub struct ToolEnv {
    pub engine: Arc<Engine>,
    /// Folder the `fs.*` tools read and write, shown to the agent.
    pub fs_root: Option<PathBuf>,
}

/// JSON Schemas for `tools/list`.
pub fn definitions() -> Value {
    let transaction_id = json!({
        "type": "string",
        "description": "The transaction_id returned by begin_transaction."
    });
    json!([
        {
            "name": "begin_transaction",
            "title": "Begin transaction",
            "description": "Start a transaction for a multi-step task. Every action you run with run_step afterwards can be rolled back automatically if a later step fails. Returns transaction_id and next_step_id (always 1).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "goal": { "type": "string", "description": "One sentence describing the task (stored for auditing)." },
                    "agent_id": { "type": "string", "description": "Optional name of the agent or app." }
                },
                "additionalProperties": false
            },
            "annotations": { "readOnlyHint": false, "destructiveHint": false, "idempotentHint": false, "openWorldHint": false }
        },
        {
            "name": "run_step",
            "title": "Run a step",
            "description": "Run one tool call inside the transaction. step_id must be the next_step_id from the previous response. If the step fails, AgentTx undoes the affected steps and returns ROLLBACK_TRIGGERED with a one-line hint and the step to resume from. Call list_tools to see which tools exist. Use \"${steps.<step_id>.<field>}\" inside arguments to reuse an earlier step's output.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "transaction_id": transaction_id,
                    "step_id": { "type": "integer", "minimum": 1, "description": "Use next_step_id from the previous response." },
                    "tool": { "type": "string", "description": "Tool name, for example kv.put, record.insert, fs.write or effect.stage." },
                    "arguments": { "type": "object", "description": "Arguments for the tool, as a JSON object.", "additionalProperties": true },
                    "reasoning": { "type": "string", "description": "Short explanation of why you run this step." }
                },
                "required": ["transaction_id", "step_id", "tool"],
                "additionalProperties": false
            },
            "annotations": { "readOnlyHint": false, "destructiveHint": false, "idempotentHint": false, "openWorldHint": false }
        },
        {
            "name": "commit_transaction",
            "title": "Commit transaction",
            "description": "Finish the task: make all step changes permanent and send any staged emails or webhooks. Call this only when every step succeeded.",
            "inputSchema": {
                "type": "object",
                "properties": { "transaction_id": transaction_id },
                "required": ["transaction_id"],
                "additionalProperties": false
            },
            "annotations": { "readOnlyHint": false, "destructiveHint": false, "idempotentHint": false, "openWorldHint": true }
        },
        {
            "name": "rollback_transaction",
            "title": "Roll back transaction",
            "description": "Undo work. With to_step, undo that step and everything after it and keep the transaction open. Without to_step, cancel the whole transaction.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "transaction_id": transaction_id,
                    "to_step": { "type": "integer", "minimum": 1, "description": "Undo this step and all later steps. Omit to cancel everything." },
                    "reason": { "type": "string", "description": "Why you are rolling back." }
                },
                "required": ["transaction_id"],
                "additionalProperties": false
            },
            "annotations": { "readOnlyHint": false, "destructiveHint": true, "idempotentHint": false, "openWorldHint": false }
        },
        {
            "name": "get_transaction",
            "title": "Get transaction status",
            "description": "Show a transaction's status, the next step to run and the rules learned from earlier failures.",
            "inputSchema": {
                "type": "object",
                "properties": { "transaction_id": transaction_id },
                "required": ["transaction_id"],
                "additionalProperties": false
            },
            "annotations": { "readOnlyHint": true, "destructiveHint": false, "idempotentHint": true, "openWorldHint": false }
        },
        {
            "name": "list_tools",
            "title": "List AgentTx tools",
            "description": "List the tools you can run with run_step, with an example of the arguments for each.",
            "inputSchema": { "type": "object", "properties": {}, "additionalProperties": false },
            "annotations": { "readOnlyHint": true, "destructiveHint": false, "idempotentHint": true, "openWorldHint": false }
        }
    ])
}

/// Runs a tool. Returns `None` when `name` is not an AgentTx MCP tool.
pub async fn call(env: &ToolEnv, name: &str, args: &Value) -> Option<Value> {
    let result = match name {
        "begin_transaction" => begin(env, args).await,
        "run_step" => run_step(env, args).await,
        "commit_transaction" => commit(env, args).await,
        "rollback_transaction" => rollback(env, args).await,
        "get_transaction" => get(env, args).await,
        "list_tools" => Ok(list_tools(env)),
        _ => return None,
    };
    Some(result.unwrap_or_else(|message| {
        let structured = json!({ "error": &message });
        tool_result(format!("✕ {message}"), structured, true)
    }))
}

fn tool_result(text: String, structured: Value, is_error: bool) -> Value {
    json!({
        "content": [{ "type": "text", "text": text }],
        "structuredContent": structured,
        "isError": is_error
    })
}

fn str_arg<'a>(args: &'a Value, name: &str) -> Result<&'a str, String> {
    args.get(name)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| format!("missing required argument `{name}` (text)"))
}

/// Accepts integers and numeric strings (models sometimes quote numbers).
fn step_arg(args: &Value, name: &str) -> Result<u32, String> {
    let value = args
        .get(name)
        .ok_or_else(|| format!("missing required argument `{name}` (whole number)"))?;
    value
        .as_u64()
        .or_else(|| value.as_str().and_then(|s| s.trim().parse().ok()))
        .and_then(|n| u32::try_from(n).ok())
        .filter(|n| *n > 0)
        .ok_or_else(|| format!("`{name}` must be a whole number of 1 or more"))
}

async fn begin(env: &ToolEnv, args: &Value) -> Result<Value, String> {
    let agent_id = args
        .get("agent_id")
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty())
        .unwrap_or("mcp-agent");
    let mut metadata = HashMap::new();
    if let Some(goal) = args.get("goal").and_then(Value::as_str) {
        metadata.insert("goal".to_owned(), goal.chars().take(1000).collect());
    }
    let outcome = env
        .engine
        .begin(agent_id, metadata, None)
        .await
        .map_err(|e| e.to_string())?;
    let text = format!(
        "Transaction started.\n\ntransaction_id: {}\nnext_step_id: {}\n\nRun each action with run_step using step_id = next_step_id. Call list_tools if you have not seen the available tools yet. When the whole task has succeeded, call commit_transaction.",
        outcome.tx_id, outcome.next_step
    );
    Ok(tool_result(
        text,
        json!({ "transaction_id": outcome.tx_id, "next_step_id": outcome.next_step }),
        false,
    ))
}

async fn run_step(env: &ToolEnv, args: &Value) -> Result<Value, String> {
    let tx_id = str_arg(args, "transaction_id")?.to_owned();
    let step_id = step_arg(args, "step_id")?;
    let tool = str_arg(args, "tool")?.to_owned();
    let arguments = match args.get("arguments") {
        None | Some(Value::Null) => json!({}),
        Some(Value::String(text)) => serde_json::from_str::<Value>(text)
            .map_err(|e| format!("`arguments` is not valid JSON: {e}"))?,
        Some(value) => value.clone(),
    };
    if !arguments.is_object() {
        return Err(
            "`arguments` must be a JSON object, for example {\"key\": \"customer\"}".into(),
        );
    }
    let reasoning = args
        .get("reasoning")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();

    let outcome = env
        .engine
        .execute_step(StepRequest {
            tx_id,
            step_id,
            tool_name: tool.clone(),
            arguments_json: arguments.to_string(),
            raw_context: reasoning,
        })
        .await
        .map_err(|e| e.to_string())?;

    let hint = outcome.hint.as_ref().map(|h| h.text.clone());
    let status = match outcome.status {
        StepStatus::Success => "SUCCESS",
        StepStatus::RollbackTriggered => "ROLLBACK_TRIGGERED",
        StepStatus::Failed => "FAILED",
    };
    let structured = json!({
        "status": status,
        "step_id": step_id,
        "tool": tool,
        "output": outcome.output,
        "clean_hint": hint,
        "error_category": outcome.hint.as_ref().map(|h| h.category.as_str()),
        "strategy": outcome.strategy.as_str(),
        "rollback_to_step": outcome.rollback_to_step,
        "next_step_id": outcome.next_step,
        "context_reset": outcome.context_reset,
        "constraints": outcome.constraints,
        "abort_reason": outcome.abort_reason,
    });

    let text = match outcome.status {
        StepStatus::Success => format!(
            "✓ Step {step_id} ({tool}) succeeded.\nOutput: {}\n\nNext: run_step with step_id {} — or commit_transaction if the task is done.",
            outcome
                .output
                .as_ref()
                .map_or_else(|| "null".to_owned(), Value::to_string),
            outcome.next_step
        ),
        StepStatus::RollbackTriggered => {
            let mut text = format!(
                "↺ Step {step_id} ({tool}) failed, so AgentTx rolled back ({}).\n{}\n\n",
                outcome.strategy.as_str(),
                hint.as_deref().unwrap_or("Hint: no details available.")
            );
            if outcome.context_reset {
                text.push_str("Every step was undone. Forget the results of earlier steps and plan the task again from step 1.\n");
            } else {
                text.push_str(&format!(
                    "Steps {} to {step_id} were undone. Run them again, fixing the problem the hint describes.\n",
                    outcome.rollback_to_step
                ));
            }
            text.push_str(&format!(
                "Resume with run_step step_id {}.",
                outcome.next_step
            ));
            if !outcome.constraints.is_empty() {
                text.push_str("\n\nRules learned from failures in this transaction:");
                for constraint in &outcome.constraints {
                    text.push_str(&format!("\n- {constraint}"));
                }
            }
            text
        }
        StepStatus::Failed => format!(
            "✕ Step {step_id} ({tool}) failed and the transaction was stopped: {}.\n{}\n\nAll changes were undone and no staged emails or webhooks were sent. Start a new transaction to try a different approach.",
            outcome
                .abort_reason
                .as_deref()
                .unwrap_or("the rollback budget is used up"),
            hint.as_deref().unwrap_or_default()
        ),
    };
    Ok(tool_result(
        text,
        structured,
        outcome.status == StepStatus::Failed,
    ))
}

async fn commit(env: &ToolEnv, args: &Value) -> Result<Value, String> {
    let tx_id = str_arg(args, "transaction_id")?;
    let outcome = env.engine.commit(tx_id).await.map_err(|e| e.to_string())?;
    let mut text = format!(
        "✓ Transaction committed. {} step(s) are now permanent.",
        outcome.steps_committed
    );
    if outcome.effects_dispatched > 0 {
        text.push_str(&format!(
            " {} staged email/webhook effect(s) were sent.",
            outcome.effects_dispatched
        ));
    }
    for error in &outcome.dispatch_errors {
        text.push_str(&format!("\nWarning: {error}"));
    }
    Ok(tool_result(
        text,
        json!({
            "committed": true,
            "steps_committed": outcome.steps_committed,
            "effects_dispatched": outcome.effects_dispatched,
            "dispatch_errors": outcome.dispatch_errors,
        }),
        false,
    ))
}

async fn rollback(env: &ToolEnv, args: &Value) -> Result<Value, String> {
    let tx_id = str_arg(args, "transaction_id")?;
    let to_step = match args.get("to_step") {
        None | Some(Value::Null) => None,
        Some(_) => Some(step_arg(args, "to_step")?),
    };
    let reason = args
        .get("reason")
        .and_then(Value::as_str)
        .unwrap_or("requested by the agent");
    let outcome = env
        .engine
        .rollback(tx_id, to_step, reason)
        .await
        .map_err(|e| e.to_string())?;
    let mut text = if outcome.aborted {
        format!(
            "Transaction cancelled. Everything was undone ({} undo action(s) ran) and nothing was sent.",
            outcome.compensations_run
        )
    } else {
        format!(
            "Rolled back. Step {} and later steps were undone. Resume with run_step step_id {}.",
            to_step.unwrap_or(outcome.next_step),
            outcome.next_step
        )
    };
    for failure in &outcome.compensation_failures {
        text.push_str(&format!("\nWarning: could not undo {failure}"));
    }
    Ok(tool_result(
        text,
        json!({
            "rolled_back": true,
            "aborted": outcome.aborted,
            "next_step_id": outcome.next_step,
            "compensations_run": outcome.compensations_run,
            "compensation_errors": outcome.compensation_failures,
        }),
        false,
    ))
}

async fn get(env: &ToolEnv, args: &Value) -> Result<Value, String> {
    let tx_id = str_arg(args, "transaction_id")?;
    let view = env.engine.get(tx_id).await.map_err(|e| e.to_string())?;
    let mut text = format!(
        "Transaction {}\nstatus: {}\nnext_step_id: {}\nsteps completed: {}\nstaged emails/webhooks waiting for commit: {}",
        view.tx_id,
        view.status.as_str(),
        view.next_step,
        view.steps_completed,
        view.pending_effects
    );
    if !view.constraints.is_empty() {
        text.push_str("\nRules learned from failures:");
        for constraint in &view.constraints {
            text.push_str(&format!("\n- {constraint}"));
        }
    }
    Ok(tool_result(
        text,
        json!({
            "transaction_id": view.tx_id,
            "status": view.status.as_str(),
            "next_step_id": view.next_step,
            "steps_completed": view.steps_completed,
            "pending_effects": view.pending_effects,
            "global_resets": view.global_resets,
            "constraints": view.constraints,
        }),
        false,
    ))
}

/// Plain-language description and example arguments for built-in tools.
fn builtin_doc(name: &str) -> Option<(&'static str, Value)> {
    Some(match name {
        "kv.put" => (
            "Save a JSON value under a key.",
            json!({ "key": "customer", "value": { "name": "Acme" } }),
        ),
        "kv.get" => (
            "Read a value saved with kv.put.",
            json!({ "key": "customer" }),
        ),
        "kv.delete" => ("Delete a saved value.", json!({ "key": "customer" })),
        "record.insert" => (
            "Insert a row into a table. Ids must be unique, and `references` checks that linked rows exist.",
            json!({ "table": "invoices", "id": "inv-1", "fields": { "customer_id": "c-1", "amount": 120 }, "references": { "customer_id": "customers" } }),
        ),
        "record.get" => (
            "Read a row by table and id.",
            json!({ "table": "customers", "id": "c-1" }),
        ),
        "fs.write" => (
            "Write a text file inside the AgentTx workspace folder. Undone automatically on rollback.",
            json!({ "path": "notes/plan.md", "contents": "# Plan" }),
        ),
        "fs.read" => (
            "Read a text file from the AgentTx workspace folder.",
            json!({ "path": "notes/plan.md" }),
        ),
        "effect.stage" => (
            "Queue an email or webhook. It is only sent after commit_transaction, and dropped on rollback.",
            json!({ "channel": "email", "payload": { "to": "team@example.com", "subject": "Report ready" } }),
        ),
        _ => return None,
    })
}

fn list_tools(env: &ToolEnv) -> Value {
    let mut text = String::from("Tools you can run with run_step:\n");
    let mut tools = Vec::new();
    for name in env.engine.tools().names() {
        let (description, example) = builtin_doc(&name)
            .map(|(d, e)| (d.to_owned(), e))
            .unwrap_or_else(|| ("Custom tool.".to_owned(), json!({})));
        text.push_str(&format!(
            "\n- {name}: {description}\n  arguments: {example}"
        ));
        tools.push(
            json!({ "name": name, "description": description, "example_arguments": example }),
        );
    }
    text.push_str("\n\nTip: reuse an earlier step's output with \"${steps.<step_id>.<field>}\", for example {\"customer_id\": \"${steps.1.id}\"}.");
    if let Some(root) = &env.fs_root {
        text.push_str(&format!(
            "\nfs.* tools work inside this folder: {}",
            root.display()
        ));
    }
    tool_result(
        text,
        json!({
            "tools": tools,
            "fs_root": env.fs_root.as_ref().map(|p| p.display().to_string()),
        }),
        false,
    )
}
