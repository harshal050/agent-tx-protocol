//! Model Context Protocol (MCP) server over stdio.
//!
//! Lets MCP clients such as Claude Code, Codex, Claude Desktop, Cursor,
//! VS Code and Gemini CLI use AgentTx transactions as tools. Messages are
//! newline-delimited JSON-RPC 2.0 on stdin/stdout; logs go to stderr.

pub mod tools;

use std::path::PathBuf;
use std::sync::Arc;

use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

use crate::engine::Engine;
use tools::ToolEnv;

/// Protocol versions this server speaks, newest first.
pub const SUPPORTED_PROTOCOL_VERSIONS: &[&str] =
    &["2025-11-25", "2025-06-18", "2025-03-26", "2024-11-05"];

/// Workflow instructions sent to the client at initialization.
pub const INSTRUCTIONS: &str = "AgentTx makes multi-step work safe to undo. \
1) Call begin_transaction. \
2) Call list_tools once to see what you can run. \
3) Do each action with run_step, always using next_step_id from the previous response. \
4) If a step returns ROLLBACK_TRIGGERED, read the hint and continue from next_step_id — the steps from rollback_to_step onward were undone and must be redone. \
5) When the whole task succeeded, call commit_transaction; to cancel, call rollback_transaction. \
Reuse earlier outputs in arguments with \"${steps.<step_id>.<field>}\".";

const PARSE_ERROR: i64 = -32700;
const INVALID_REQUEST: i64 = -32600;
const METHOD_NOT_FOUND: i64 = -32601;
const INVALID_PARAMS: i64 = -32602;

/// A transport-independent MCP request handler.
pub struct McpServer {
    env: ToolEnv,
}

impl McpServer {
    pub fn new(engine: Arc<Engine>, fs_root: Option<PathBuf>) -> Self {
        Self {
            env: ToolEnv { engine, fs_root },
        }
    }

    /// Handles one JSON-RPC message and returns the response to send, if any.
    /// Notifications and client responses produce no output.
    pub async fn handle_message(&self, raw: &str) -> Option<Value> {
        let message: Value = match serde_json::from_str(raw) {
            Ok(value) => value,
            Err(error) => {
                return Some(error_response(
                    Value::Null,
                    PARSE_ERROR,
                    &format!("parse error: {error}"),
                ));
            }
        };
        let Some(object) = message.as_object() else {
            return Some(error_response(
                Value::Null,
                INVALID_REQUEST,
                "expected a JSON-RPC request object",
            ));
        };
        // Responses to server-initiated requests carry no method; we send none.
        let method = object.get("method").and_then(Value::as_str)?;
        let id = object.get("id").cloned();
        let params = object.get("params").cloned().unwrap_or_else(|| json!({}));

        let result = match method {
            "initialize" => Ok(initialize_result(&params)),
            "ping" => Ok(json!({})),
            "tools/list" => Ok(json!({ "tools": tools::definitions() })),
            "tools/call" => self.call_tool(&params).await,
            "resources/list" => Ok(json!({ "resources": [] })),
            "resources/templates/list" => Ok(json!({ "resourceTemplates": [] })),
            "prompts/list" => Ok(json!({ "prompts": [] })),
            _ if id.is_none() => return None,
            _ => Err((METHOD_NOT_FOUND, format!("method not found: {method}"))),
        };

        let id = id?;
        Some(match result {
            Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
            Err((code, message)) => error_response(id, code, &message),
        })
    }

    async fn call_tool(&self, params: &Value) -> Result<Value, (i64, String)> {
        let name = params
            .get("name")
            .and_then(Value::as_str)
            .ok_or((INVALID_PARAMS, "tools/call requires `name`".to_owned()))?;
        let arguments = params
            .get("arguments")
            .cloned()
            .unwrap_or_else(|| json!({}));
        tools::call(&self.env, name, &arguments)
            .await
            .ok_or_else(|| (INVALID_PARAMS, format!("unknown tool: {name}")))
    }
}

fn initialize_result(params: &Value) -> Value {
    let requested = params.get("protocolVersion").and_then(Value::as_str);
    let version = requested
        .filter(|v| SUPPORTED_PROTOCOL_VERSIONS.contains(v))
        .unwrap_or(SUPPORTED_PROTOCOL_VERSIONS[0]);
    json!({
        "protocolVersion": version,
        "capabilities": { "tools": { "listChanged": false } },
        "serverInfo": {
            "name": "agenttx",
            "title": "AgentTx",
            "version": env!("CARGO_PKG_VERSION"),
            "websiteUrl": "https://agenttx.site"
        },
        "instructions": INSTRUCTIONS
    })
}

fn error_response(id: Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

/// Serves MCP over stdin/stdout until stdin closes.
pub async fn serve_stdio(server: McpServer) -> std::io::Result<()> {
    let mut lines = BufReader::new(tokio::io::stdin()).lines();
    let mut stdout = tokio::io::stdout();
    while let Some(line) = lines.next_line().await? {
        if line.trim().is_empty() {
            continue;
        }
        if let Some(response) = server.handle_message(&line).await {
            let mut bytes = serde_json::to_vec(&response).map_err(std::io::Error::other)?;
            bytes.push(b'\n');
            stdout.write_all(&bytes).await?;
            stdout.flush().await?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::EngineConfig;
    use crate::engine::{ToolRegistry, builtin_tools};
    use crate::ledger::{OutboxDispatcher, UndoRegistry};
    use crate::storage::{RocksStore, StoreOptions};

    struct Fixture {
        server: McpServer,
        _db: tempfile::TempDir,
        _sandbox: tempfile::TempDir,
    }

    fn fixture() -> Fixture {
        let db = tempfile::tempdir().unwrap();
        let sandbox = tempfile::tempdir().unwrap();
        let store =
            Arc::new(RocksStore::open(db.path().join("rocks"), StoreOptions::default()).unwrap());
        let tools = ToolRegistry::new();
        builtin_tools::register_builtin_tools(&tools, sandbox.path());
        let undo = UndoRegistry::new();
        builtin_tools::register_builtin_undo_factories(&undo);
        let engine = Arc::new(Engine::new(
            store,
            tools,
            undo,
            Arc::new(OutboxDispatcher::new(db.path().join("outbox.jsonl"))),
            EngineConfig::default(),
        ));
        Fixture {
            server: McpServer::new(engine, Some(sandbox.path().to_path_buf())),
            _db: db,
            _sandbox: sandbox,
        }
    }

    async fn request(server: &McpServer, id: u64, method: &str, params: Value) -> Value {
        let raw =
            json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }).to_string();
        server
            .handle_message(&raw)
            .await
            .expect("requests get a response")
    }

    async fn call(server: &McpServer, id: u64, name: &str, arguments: Value) -> Value {
        let response = request(
            server,
            id,
            "tools/call",
            json!({ "name": name, "arguments": arguments }),
        )
        .await;
        response["result"].clone()
    }

    #[tokio::test]
    async fn initialize_negotiates_protocol_version() {
        let f = fixture();
        let known = request(&f.server, 1, "initialize", json!({ "protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": { "name": "test", "version": "1" } })).await;
        assert_eq!(known["result"]["protocolVersion"], "2025-06-18");
        assert_eq!(known["result"]["serverInfo"]["name"], "agenttx");
        assert!(known["result"]["capabilities"]["tools"].is_object());
        assert!(
            known["result"]["instructions"]
                .as_str()
                .unwrap()
                .contains("begin_transaction")
        );

        let unknown = request(
            &f.server,
            2,
            "initialize",
            json!({ "protocolVersion": "1999-01-01" }),
        )
        .await;
        assert_eq!(
            unknown["result"]["protocolVersion"],
            SUPPORTED_PROTOCOL_VERSIONS[0]
        );

        let notification =
            json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }).to_string();
        assert!(f.server.handle_message(&notification).await.is_none());
        assert_eq!(
            request(&f.server, 3, "ping", json!({})).await["result"],
            json!({})
        );
    }

    #[tokio::test]
    async fn lists_tools_with_schemas() {
        let f = fixture();
        let response = request(&f.server, 1, "tools/list", json!({})).await;
        let tools = response["result"]["tools"].as_array().unwrap();
        let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
        assert_eq!(
            names,
            [
                "begin_transaction",
                "run_step",
                "commit_transaction",
                "rollback_transaction",
                "get_transaction",
                "list_tools"
            ]
        );
        for tool in tools {
            assert_eq!(tool["inputSchema"]["type"], "object");
            assert!(tool["description"].as_str().unwrap().len() > 20);
        }
    }

    #[tokio::test]
    async fn full_transaction_with_dependency_jump() {
        let f = fixture();
        let begun = call(
            &f.server,
            1,
            "begin_transaction",
            json!({ "goal": "invoice a customer" }),
        )
        .await;
        assert_eq!(begun["isError"], false);
        let tx = begun["structuredContent"]["transaction_id"]
            .as_str()
            .unwrap()
            .to_owned();

        let listed = call(&f.server, 2, "list_tools", json!({})).await;
        assert!(
            listed["content"][0]["text"]
                .as_str()
                .unwrap()
                .contains("record.insert")
        );

        let stored = call(
            &f.server,
            3,
            "run_step",
            json!({
                "transaction_id": tx, "step_id": 1, "tool": "kv.put",
                "arguments": { "key": "customer", "value": "c-404" }
            }),
        )
        .await;
        assert_eq!(stored["structuredContent"]["status"], "SUCCESS");

        let failed = call(&f.server, 4, "run_step", json!({
            "transaction_id": tx, "step_id": "2", "tool": "record.insert",
            "arguments": { "table": "invoices", "id": "inv-1", "fields": { "customer_id": "${steps.1.value}" }, "references": { "customer_id": "customers" } }
        })).await;
        assert_eq!(failed["isError"], false);
        assert_eq!(failed["structuredContent"]["status"], "ROLLBACK_TRIGGERED");
        assert_eq!(failed["structuredContent"]["strategy"], "DEPENDENCY_JUMP");
        assert_eq!(failed["structuredContent"]["next_step_id"], 1);
        let text = failed["content"][0]["text"].as_str().unwrap();
        assert!(
            text.contains("Foreign key constraint failed for 'customer_id'"),
            "{text}"
        );
        assert!(text.contains("Resume with run_step step_id 1"), "{text}");

        call(&f.server, 5, "run_step", json!({ "transaction_id": tx, "step_id": 1, "tool": "record.insert", "arguments": { "table": "customers", "id": "c-1" } })).await;
        let invoice = call(&f.server, 6, "run_step", json!({
            "transaction_id": tx, "step_id": 2, "tool": "record.insert",
            "arguments": { "table": "invoices", "id": "inv-1", "fields": { "customer_id": "${steps.1.id}" }, "references": { "customer_id": "customers" } }
        })).await;
        assert_eq!(invoice["structuredContent"]["status"], "SUCCESS");

        let status = call(
            &f.server,
            7,
            "get_transaction",
            json!({ "transaction_id": tx }),
        )
        .await;
        assert_eq!(status["structuredContent"]["next_step_id"], 3);

        let committed = call(
            &f.server,
            8,
            "commit_transaction",
            json!({ "transaction_id": tx }),
        )
        .await;
        assert_eq!(committed["structuredContent"]["steps_committed"], 2);
    }

    #[tokio::test]
    async fn tool_errors_are_results_and_protocol_errors_are_rpc_errors() {
        let f = fixture();
        let wrong_step = call(
            &f.server,
            1,
            "run_step",
            json!({ "transaction_id": "missing", "step_id": 1, "tool": "kv.put" }),
        )
        .await;
        assert_eq!(wrong_step["isError"], true);
        assert!(
            wrong_step["content"][0]["text"]
                .as_str()
                .unwrap()
                .contains("not found")
        );

        let bad_args = call(
            &f.server,
            2,
            "run_step",
            json!({ "transaction_id": "t", "step_id": 0, "tool": "kv.put" }),
        )
        .await;
        assert_eq!(bad_args["isError"], true);

        let unknown_tool = request(&f.server, 3, "tools/call", json!({ "name": "nope" })).await;
        assert_eq!(unknown_tool["error"]["code"], INVALID_PARAMS);

        let unknown_method = request(&f.server, 4, "sampling/createMessage", json!({})).await;
        assert_eq!(unknown_method["error"]["code"], METHOD_NOT_FOUND);

        let garbage = f.server.handle_message("{not json").await.unwrap();
        assert_eq!(garbage["error"]["code"], PARSE_ERROR);

        let client_response = json!({ "jsonrpc": "2.0", "id": 9, "result": {} }).to_string();
        assert!(f.server.handle_message(&client_response).await.is_none());
    }
}
