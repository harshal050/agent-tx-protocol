//! Built-in reference tools.
//!
//! | Tool            | Effect class          | Rollback mechanism              |
//! |-----------------|-----------------------|---------------------------------|
//! | `kv.put/get/delete` | transactional state | RocksDB journal snapshots     |
//! | `record.insert/get` | transactional state with PK/FK checks | journal snapshots |
//! | `fs.write`      | reversible external   | Saga undo (restore prior bytes) |
//! | `fs.read`       | read-only             | —                               |
//! | `effect.stage`  | non-reversible        | staged until commit             |
//!
//! Error messages deliberately mirror the formats of real systems (PostgreSQL,
//! POSIX, serde) so the deterministic error cleaner produces precise hints.

use std::io::ErrorKind;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{Map, Value, json};

use super::executor::{Tool, ToolContext, ToolError, ToolRegistry};
use crate::errors::{AgentTxError, Result};
use crate::ledger::{UndoAction, UndoRegistry};

/// Registers every built-in tool; `fs_root` confines the `fs.*` tools.
pub fn register_builtin_tools(registry: &ToolRegistry, fs_root: impl Into<PathBuf>) {
    let fs_root = fs_root.into();
    registry.register(Arc::new(KvPut));
    registry.register(Arc::new(KvGet));
    registry.register(Arc::new(KvDelete));
    registry.register(Arc::new(RecordInsert));
    registry.register(Arc::new(RecordGet));
    registry.register(Arc::new(FsWrite { root: fs_root.clone() }));
    registry.register(Arc::new(FsRead { root: fs_root }));
    registry.register(Arc::new(EffectStage));
}

/// Registers crash-recovery factories for built-in undo actions.
pub fn register_builtin_undo_factories(registry: &UndoRegistry) {
    registry.register(FileRestoreUndo::KIND, |payload| {
        Ok(Arc::new(FileRestoreUndo::from_payload(payload)?) as Arc<dyn UndoAction>)
    });
}

// ---------------------------------------------------------------------------
// Argument helpers
// ---------------------------------------------------------------------------

fn type_name(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

fn required<'a>(args: &'a Value, name: &str) -> std::result::Result<&'a Value, ToolError> {
    match args.get(name) {
        Some(Value::Null) | None => Err(ToolError::failed(format!("missing field `{name}`"))),
        Some(v) => Ok(v),
    }
}

fn str_arg<'a>(args: &'a Value, name: &str) -> std::result::Result<&'a str, ToolError> {
    let value = required(args, name)?;
    value.as_str().ok_or_else(|| {
        ToolError::failed(format!(
            "invalid type: {} for field `{name}`, expected a string",
            type_name(value)
        ))
    })
}

/// Accepts strings or numbers as identifiers.
fn id_arg(args: &Value, name: &str) -> std::result::Result<String, ToolError> {
    match required(args, name)? {
        Value::String(s) if !s.is_empty() => Ok(s.clone()),
        Value::Number(n) => Ok(n.to_string()),
        other => Err(ToolError::failed(format!(
            "invalid type: {} for field `{name}`, expected a string or number",
            type_name(other)
        ))),
    }
}

fn identifier(value: &str, what: &str) -> std::result::Result<(), ToolError> {
    if !value.is_empty() && value.len() <= 128 && value.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') {
        Ok(())
    } else {
        Err(ToolError::failed(format!(
            "invalid {what} `{value}`: expected 1-128 chars of [A-Za-z0-9_]"
        )))
    }
}

fn decode_json(bytes: &[u8]) -> std::result::Result<Value, ToolError> {
    serde_json::from_slice(bytes).map_err(|e| ToolError::Internal(e.into()))
}

// ---------------------------------------------------------------------------
// kv.*
// ---------------------------------------------------------------------------

struct KvPut;

#[async_trait]
impl Tool for KvPut {
    fn name(&self) -> &str {
        "kv.put"
    }

    async fn execute(&self, ctx: &ToolContext, args: Value) -> std::result::Result<Value, ToolError> {
        let key = str_arg(&args, "key")?;
        let value = args.get("value").cloned().unwrap_or(Value::Null);
        ctx.store()
            .put(&format!("kv/{key}"), &serde_json::to_vec(&value).map_err(AgentTxError::from)?)?;
        Ok(json!({ "key": key, "value": value }))
    }
}

struct KvGet;

#[async_trait]
impl Tool for KvGet {
    fn name(&self) -> &str {
        "kv.get"
    }

    async fn execute(&self, ctx: &ToolContext, args: Value) -> std::result::Result<Value, ToolError> {
        let key = str_arg(&args, "key")?;
        match ctx.store().get(&format!("kv/{key}"))? {
            Some(bytes) => Ok(json!({ "key": key, "value": decode_json(&bytes)? })),
            None => Err(ToolError::failed(format!(
                "no row with key={key} in table \"kv\""
            ))),
        }
    }
}

struct KvDelete;

#[async_trait]
impl Tool for KvDelete {
    fn name(&self) -> &str {
        "kv.delete"
    }

    async fn execute(&self, ctx: &ToolContext, args: Value) -> std::result::Result<Value, ToolError> {
        let key = str_arg(&args, "key")?;
        let raw = format!("kv/{key}");
        let existed = ctx.store().get(&raw)?.is_some();
        if existed {
            ctx.store().delete(&raw)?;
        }
        Ok(json!({ "key": key, "deleted": existed }))
    }
}

// ---------------------------------------------------------------------------
// record.*
// ---------------------------------------------------------------------------

fn record_key(table: &str, id: &str) -> String {
    format!("records/{table}/{id}")
}

/// Inserts a row with primary-key uniqueness and declared foreign keys.
///
/// Arguments: `{ "table", "id", "fields"?: {..}, "references"?: {"field": "table"} }`.
struct RecordInsert;

#[async_trait]
impl Tool for RecordInsert {
    fn name(&self) -> &str {
        "record.insert"
    }

    async fn execute(&self, ctx: &ToolContext, args: Value) -> std::result::Result<Value, ToolError> {
        let table = str_arg(&args, "table")?;
        identifier(table, "table")?;
        let id = id_arg(&args, "id")?;
        let fields = match args.get("fields") {
            None | Some(Value::Null) => Map::new(),
            Some(Value::Object(map)) => map.clone(),
            Some(other) => {
                return Err(ToolError::failed(format!(
                    "invalid type: {} for field `fields`, expected an object",
                    type_name(other)
                )));
            }
        };
        let store = ctx.store();
        if store.get(&record_key(table, &id))?.is_some() {
            return Err(ToolError::failed(format!(
                "ERROR: duplicate key value violates unique constraint \"{table}_pkey\"\nDETAIL: Key (id)=({id}) already exists."
            )));
        }

        if let Some(references) = args.get("references").and_then(Value::as_object) {
            for (field, ref_table) in references {
                let ref_table = ref_table.as_str().ok_or_else(|| {
                    ToolError::failed(format!(
                        "invalid type: {} for references.{field}, expected a string",
                        type_name(ref_table)
                    ))
                })?;
                identifier(ref_table, "referenced table")?;
                let ref_id = match fields.get(field) {
                    None | Some(Value::Null) => {
                        return Err(ToolError::failed(format!(
                            "ERROR: null value in column \"{field}\" of relation \"{table}\" violates not-null constraint"
                        )));
                    }
                    Some(Value::String(s)) => s.clone(),
                    Some(other) => other.to_string(),
                };
                if store.get(&record_key(ref_table, &ref_id))?.is_none() {
                    return Err(ToolError::failed(format!(
                        "ERROR: insert or update on table \"{table}\" violates foreign key constraint \"{table}_{field}_fkey\"\nDETAIL: Key ({field})=({ref_id}) is not present in table \"{ref_table}\"."
                    )));
                }
            }
        }

        let mut row = fields;
        row.insert("id".into(), args["id"].clone());
        let row = Value::Object(row);
        store.put(
            &record_key(table, &id),
            &serde_json::to_vec(&row).map_err(AgentTxError::from)?,
        )?;
        let mut output = row;
        output["table"] = Value::String(table.to_owned());
        Ok(output)
    }
}

struct RecordGet;

#[async_trait]
impl Tool for RecordGet {
    fn name(&self) -> &str {
        "record.get"
    }

    async fn execute(&self, ctx: &ToolContext, args: Value) -> std::result::Result<Value, ToolError> {
        let table = str_arg(&args, "table")?;
        identifier(table, "table")?;
        let id = id_arg(&args, "id")?;
        match ctx.store().get(&record_key(table, &id))? {
            Some(bytes) => decode_json(&bytes),
            None => Err(ToolError::failed(format!(
                "no row with id={id} in table \"{table}\""
            ))),
        }
    }
}

// ---------------------------------------------------------------------------
// fs.*
// ---------------------------------------------------------------------------

/// Resolves `relative` under `root`, rejecting absolute paths, `..`, and
/// symlinks escaping the sandbox. Creates parent directories.
async fn sandboxed_path(root: &Path, relative: &str) -> std::result::Result<PathBuf, ToolError> {
    let escape = || ToolError::failed(format!("path '{relative}' escapes the sandbox"));
    let rel = Path::new(relative);
    if relative.is_empty() || !rel.components().all(|c| matches!(c, Component::Normal(_))) {
        return Err(escape());
    }
    tokio::fs::create_dir_all(root).await.map_err(AgentTxError::from)?;
    let canonical_root = tokio::fs::canonicalize(root).await.map_err(AgentTxError::from)?;
    let full = canonical_root.join(rel);
    let parent = full.parent().ok_or_else(escape)?;
    tokio::fs::create_dir_all(parent)
        .await
        .map_err(|e| ToolError::failed(format!("{e}: '{relative}'")))?;
    let canonical_parent = tokio::fs::canonicalize(parent).await.map_err(AgentTxError::from)?;
    if !canonical_parent.starts_with(&canonical_root) {
        return Err(escape());
    }
    if let Ok(meta) = tokio::fs::symlink_metadata(&full).await
        && meta.file_type().is_symlink()
    {
        return Err(escape());
    }
    Ok(full)
}

async fn write_atomically(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".agenttx-tmp");
    let tmp = PathBuf::from(tmp);
    tokio::fs::write(&tmp, bytes).await?;
    tokio::fs::rename(&tmp, path).await
}

/// Writes a UTF-8 file inside the sandbox; compensated by restoring the
/// previous contents (or deleting a newly created file).
struct FsWrite {
    root: PathBuf,
}

#[async_trait]
impl Tool for FsWrite {
    fn name(&self) -> &str {
        "fs.write"
    }

    async fn execute(&self, ctx: &ToolContext, args: Value) -> std::result::Result<Value, ToolError> {
        let relative = str_arg(&args, "path")?;
        let contents = str_arg(&args, "contents")?;
        let path = sandboxed_path(&self.root, relative).await?;

        let prior = match tokio::fs::read(&path).await {
            Ok(bytes) => Some(bytes),
            Err(e) if e.kind() == ErrorKind::NotFound => None,
            Err(e) => return Err(ToolError::failed(format!("{e}: '{relative}'"))),
        };
        // Register the compensation before touching the file.
        ctx.register_undo(Arc::new(FileRestoreUndo {
            path: path.clone(),
            prior,
        }));
        write_atomically(&path, contents.as_bytes())
            .await
            .map_err(|e| ToolError::failed(format!("{e}: '{relative}'")))?;
        Ok(json!({ "path": relative, "bytes": contents.len() }))
    }
}

struct FsRead {
    root: PathBuf,
}

#[async_trait]
impl Tool for FsRead {
    fn name(&self) -> &str {
        "fs.read"
    }

    async fn execute(&self, _ctx: &ToolContext, args: Value) -> std::result::Result<Value, ToolError> {
        let relative = str_arg(&args, "path")?;
        let path = sandboxed_path(&self.root, relative).await?;
        match tokio::fs::read(&path).await {
            Ok(bytes) => Ok(json!({
                "path": relative,
                "contents": String::from_utf8_lossy(&bytes),
            })),
            Err(e) if e.kind() == ErrorKind::NotFound => Err(ToolError::failed(format!(
                "No such file or directory: '{relative}'"
            ))),
            Err(e) => Err(ToolError::failed(format!("{e}: '{relative}'"))),
        }
    }
}

/// Restores a file to its pre-transaction bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileRestoreUndo {
    pub path: PathBuf,
    /// `None` if the file did not exist (undo deletes it).
    pub prior: Option<Vec<u8>>,
}

impl FileRestoreUndo {
    pub const KIND: &'static str = "fs.restore_file";

    pub fn from_payload(payload: &Value) -> Result<Self> {
        let invalid = |reason: &str| AgentTxError::Compensation {
            action: Self::KIND.into(),
            reason: reason.into(),
        };
        let path = payload
            .get("path")
            .and_then(Value::as_str)
            .ok_or_else(|| invalid("payload missing `path`"))?;
        let prior = match payload.get("prior_hex") {
            None | Some(Value::Null) => None,
            Some(Value::String(hex)) => Some(hex_decode(hex).ok_or_else(|| invalid("bad hex"))?),
            Some(_) => return Err(invalid("`prior_hex` must be a string or null")),
        };
        Ok(Self {
            path: PathBuf::from(path),
            prior,
        })
    }
}

#[async_trait]
impl UndoAction for FileRestoreUndo {
    fn kind(&self) -> &str {
        Self::KIND
    }

    fn describe(&self) -> String {
        match self.prior {
            Some(_) => format!("restore {}", self.path.display()),
            None => format!("delete {}", self.path.display()),
        }
    }

    fn payload(&self) -> Value {
        json!({
            "path": self.path.to_string_lossy(),
            "prior_hex": self.prior.as_deref().map(hex_encode),
        })
    }

    async fn undo(&self) -> std::result::Result<(), String> {
        match &self.prior {
            Some(bytes) => write_atomically(&self.path, bytes)
                .await
                .map_err(|e| format!("{e}: '{}'", self.path.display())),
            None => match tokio::fs::remove_file(&self.path).await {
                Ok(()) => Ok(()),
                Err(e) if e.kind() == ErrorKind::NotFound => Ok(()),
                Err(e) => Err(format!("{e}: '{}'", self.path.display())),
            },
        }
    }
}

fn hex_encode(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push(DIGITS[(b >> 4) as usize] as char);
        out.push(DIGITS[(b & 0x0f) as usize] as char);
    }
    out
}

fn hex_decode(hex: &str) -> Option<Vec<u8>> {
    if hex.len() % 2 != 0 {
        return None;
    }
    (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(hex.get(i..i + 2)?, 16).ok())
        .collect()
}

// ---------------------------------------------------------------------------
// effect.stage
// ---------------------------------------------------------------------------

/// Stages a non-reversible effect (`{ "channel", "payload", "idempotency_key"? }`).
struct EffectStage;

#[async_trait]
impl Tool for EffectStage {
    fn name(&self) -> &str {
        "effect.stage"
    }

    async fn execute(&self, ctx: &ToolContext, args: Value) -> std::result::Result<Value, ToolError> {
        let channel = str_arg(&args, "channel")?;
        identifier(channel, "channel")?;
        let payload = args.get("payload").cloned().unwrap_or(Value::Null);
        let key = match args.get("idempotency_key") {
            None | Some(Value::Null) => None,
            Some(Value::String(s)) => Some(s.clone()),
            Some(other) => {
                return Err(ToolError::failed(format!(
                    "invalid type: {} for field `idempotency_key`, expected a string",
                    type_name(other)
                )));
            }
        };
        let key = ctx.stage_effect(channel, payload, key);
        Ok(json!({ "staged": true, "channel": channel, "idempotency_key": key }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::{RocksStore, StoreOptions};

    struct Fixture {
        _db: tempfile::TempDir,
        sandbox: tempfile::TempDir,
        registry: ToolRegistry,
        store: Arc<RocksStore>,
    }

    fn fixture() -> Fixture {
        let db = tempfile::tempdir().unwrap();
        let sandbox = tempfile::tempdir().unwrap();
        let store = Arc::new(RocksStore::open(db.path(), StoreOptions::default()).unwrap());
        let registry = ToolRegistry::new();
        register_builtin_tools(&registry, sandbox.path());
        Fixture { _db: db, sandbox, registry, store }
    }

    async fn run(f: &Fixture, ctx: &ToolContext, tool: &str, args: Value) -> std::result::Result<Value, ToolError> {
        f.registry.get(tool).unwrap().execute(ctx, args).await
    }

    fn failure(result: std::result::Result<Value, ToolError>) -> String {
        match result {
            Err(ToolError::Failed(m)) => m,
            other => panic!("expected tool failure, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn kv_roundtrip() {
        let f = fixture();
        let ctx = ToolContext::new(f.store.scoped("tx").unwrap(), 1);
        run(&f, &ctx, "kv.put", json!({ "key": "a", "value": { "n": 1 } })).await.unwrap();
        let got = run(&f, &ctx, "kv.get", json!({ "key": "a" })).await.unwrap();
        assert_eq!(got["value"], json!({ "n": 1 }));
        let deleted = run(&f, &ctx, "kv.delete", json!({ "key": "a" })).await.unwrap();
        assert_eq!(deleted["deleted"], true);
        let missing = failure(run(&f, &ctx, "kv.get", json!({ "key": "a" })).await);
        assert!(missing.contains("no row with key=a"));
        let bad = failure(run(&f, &ctx, "kv.put", json!({ "key": 5 })).await);
        assert!(bad.starts_with("invalid type: number"));
    }

    #[tokio::test]
    async fn record_insert_enforces_primary_and_foreign_keys() {
        let f = fixture();
        let ctx = ToolContext::new(f.store.scoped("tx").unwrap(), 1);
        let fk = failure(
            run(&f, &ctx, "record.insert", json!({
                "table": "orders", "id": "o1",
                "fields": { "user_id": 101 }, "references": { "user_id": "users" }
            }))
            .await,
        );
        assert!(fk.contains("Key (user_id)=(101) is not present in table \"users\""), "{fk}");

        let user = run(&f, &ctx, "record.insert", json!({ "table": "users", "id": 101, "fields": { "name": "Ada" } })).await.unwrap();
        assert_eq!(user, json!({ "table": "users", "id": 101, "name": "Ada" }));

        run(&f, &ctx, "record.insert", json!({
            "table": "orders", "id": "o1",
            "fields": { "user_id": 101 }, "references": { "user_id": "users" }
        }))
        .await
        .unwrap();

        let dup = failure(run(&f, &ctx, "record.insert", json!({ "table": "users", "id": "101" })).await);
        assert!(dup.contains("Key (id)=(101) already exists"));

        let null = failure(
            run(&f, &ctx, "record.insert", json!({ "table": "orders", "id": "o2", "references": { "user_id": "users" } })).await,
        );
        assert!(null.contains("null value in column \"user_id\""));

        let got = run(&f, &ctx, "record.get", json!({ "table": "users", "id": 101 })).await.unwrap();
        assert_eq!(got["name"], "Ada");
        assert!(failure(run(&f, &ctx, "record.insert", json!({ "table": "bad-name", "id": 1 })).await).contains("invalid table"));
    }

    #[tokio::test]
    async fn fs_write_registers_restorable_undo() {
        let f = fixture();
        let ctx = ToolContext::new(f.store.scoped("tx").unwrap(), 1);
        let file = f.sandbox.path().join("notes/a.txt");

        run(&f, &ctx, "fs.write", json!({ "path": "notes/a.txt", "contents": "v1" })).await.unwrap();
        run(&f, &ctx, "fs.write", json!({ "path": "notes/a.txt", "contents": "v2" })).await.unwrap();
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "v2");
        let read = run(&f, &ctx, "fs.read", json!({ "path": "notes/a.txt" })).await.unwrap();
        assert_eq!(read["contents"], "v2");

        let undos = ctx.take_undo();
        assert_eq!(undos.len(), 2);
        // Round-trip the second action through its persisted payload.
        let rebuilt = FileRestoreUndo::from_payload(&undos[1].payload()).unwrap();
        rebuilt.undo().await.unwrap();
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "v1");
        undos[0].undo().await.unwrap();
        assert!(!file.exists());
        undos[0].undo().await.unwrap(); // idempotent
    }

    #[tokio::test]
    async fn fs_tools_reject_sandbox_escapes() {
        let f = fixture();
        let ctx = ToolContext::new(f.store.scoped("tx").unwrap(), 1);
        for path in ["../x", "/etc/passwd", "a/../../x", ""] {
            let msg = failure(run(&f, &ctx, "fs.write", json!({ "path": path, "contents": "x" })).await);
            assert!(msg.contains("escapes the sandbox"), "{path}: {msg}");
        }
        #[cfg(unix)]
        {
            let outside = tempfile::tempdir().unwrap();
            std::os::unix::fs::symlink(outside.path(), f.sandbox.path().join("link")).unwrap();
            let msg = failure(run(&f, &ctx, "fs.write", json!({ "path": "link/x", "contents": "x" })).await);
            assert!(msg.contains("escapes the sandbox"), "{msg}");
        }
        let missing = failure(run(&f, &ctx, "fs.read", json!({ "path": "nope.txt" })).await);
        assert!(missing.contains("No such file or directory: 'nope.txt'"));
        assert!(ctx.take_undo().is_empty());
    }

    #[tokio::test]
    async fn effect_stage_defers_effects() {
        let f = fixture();
        let ctx = ToolContext::new(f.store.scoped("tx").unwrap(), 4);
        let out = run(&f, &ctx, "effect.stage", json!({ "channel": "email", "payload": { "to": "a@b.c" }, "idempotency_key": "k1" })).await.unwrap();
        assert_eq!(out["idempotency_key"], "k1");
        let staged = ctx.take_staged();
        assert_eq!(staged[0].step_id, 4);
        assert_eq!(staged[0].payload["to"], "a@b.c");
    }

    #[test]
    fn hex_roundtrip() {
        let bytes = vec![0u8, 1, 0x7f, 0x80, 0xff];
        assert_eq!(hex_decode(&hex_encode(&bytes)).unwrap(), bytes);
        assert!(hex_decode("abc").is_none());
        assert!(hex_decode("zz").is_none());
    }
}
