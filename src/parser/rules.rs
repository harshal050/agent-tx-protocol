//! Ordered rule table for [`super::ErrorCleaner`].
//!
//! Rules are evaluated as one `RegexSet`; when several match, the rule listed
//! first wins, so specific rules precede generic ones. Each rule renders a
//! single-line hint from its captures and may name capture groups holding the
//! offending key (used by the dependency graph for jump rollbacks).

use regex::Captures;

use super::ErrorCategory;

/// One deterministic error-matching rule.
pub struct Rule {
    pub name: &'static str,
    pub category: ErrorCategory,
    pub pattern: &'static str,
    /// Capture groups that may hold the offending key, in preference order.
    pub key_groups: &'static [&'static str],
    pub render: fn(&Captures<'_>) -> String,
}

impl std::fmt::Debug for Rule {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Rule")
            .field("name", &self.name)
            .field("category", &self.category)
            .finish_non_exhaustive()
    }
}

/// First non-empty capture among `names`.
pub(crate) fn pick<'a>(caps: &'a Captures<'_>, names: &[&str]) -> Option<&'a str> {
    names
        .iter()
        .filter_map(|n| caps.name(n))
        .map(|m| m.as_str().trim())
        .find(|s| !s.is_empty())
}

fn get<'a>(caps: &'a Captures<'_>, names: &[&str]) -> &'a str {
    pick(caps, names).unwrap_or("unknown")
}

/// First column of a possibly composite key: `(tenant_id, user_id)` → `tenant_id`.
pub(crate) fn first_column(key: &str) -> String {
    key.trim_matches(|c| c == '(' || c == ')')
        .split(',')
        .next()
        .unwrap_or(key)
        .trim()
        .trim_matches(|c| c == '"' || c == '`' || c == '\'')
        .to_owned()
}

/// Naive English singular of a table name, schema-qualification removed.
pub(crate) fn singular(table: &str) -> String {
    let name = table.rsplit('.').next().unwrap_or(table);
    if let Some(stem) = name.strip_suffix("ies") {
        format!("{stem}y")
    } else if name.ends_with("sses") || name.ends_with("xes") || name.ends_with("ches") {
        name[..name.len() - 2].to_owned()
    } else if name.ends_with('s') && !name.ends_with("ss") {
        name[..name.len() - 1].to_owned()
    } else {
        name.to_owned()
    }
}

/// Prefixes an HTTP status pattern with common "HTTP 404" / "status: 404" forms.
macro_rules! const_format_http {
    ($suffix:literal) => {
        concat!(
            r"\b(?:HTTP(?:/\d(?:\.\d)?)?|[Ss]tatus(?: [Cc]ode)?:?|returned)\s*",
            $suffix
        )
    };
}

/// Built-in rules, most specific first.
pub static RULES: &[Rule] = &[
    // --- AgentTx protocol errors ------------------------------------------
    Rule {
        name: "unresolved_reference",
        category: ErrorCategory::UnresolvedReference,
        pattern: r"unresolved reference \$\{steps\.(?P<step>\d+)[^}]*\}: key '(?P<key>[^']+)' not present in output of step \d+",
        key_groups: &["key"],
        render: |c| {
            let (step, key) = (get(c, &["step"]), get(c, &["key"]));
            format!("Hint: Step {step} output has no '{key}'. Re-run step {step} so it returns '{key}', or fix the reference.")
        },
    },
    Rule {
        name: "unavailable_step_reference",
        category: ErrorCategory::UnresolvedReference,
        pattern: r"reference \$\{steps\.(?P<step>\d+)[^}]*\} points to a step that has not completed",
        key_groups: &[],
        render: |c| format!(
            "Hint: Step {} has not completed; only reference outputs of earlier successful steps.",
            get(c, &["step"])
        ),
    },
    Rule {
        name: "unknown_tool",
        category: ErrorCategory::UnknownTool,
        pattern: r"unknown tool '(?P<tool>[^']*)'; registered tools: (?P<tools>[^\n]*)",
        key_groups: &[],
        render: |c| format!(
            "Hint: Tool '{}' does not exist. Use one of: {}.",
            get(c, &["tool"]),
            get(c, &["tools"])
        ),
    },
    // --- SQL constraint violations ----------------------------------------
    Rule {
        name: "pg_foreign_key",
        category: ErrorCategory::ForeignKeyViolation,
        pattern: r#"Key \((?P<key>[^)]+)\)=\((?P<value>[^)]*)\) is not present in table "(?P<table>[^"]+)""#,
        key_groups: &["key"],
        render: |c| format!(
            "Hint: Foreign key constraint failed for '{}'. Ensure target {} exists before step execution.",
            first_column(get(c, &["key"])),
            singular(get(c, &["table"]))
        ),
    },
    Rule {
        name: "mysql_foreign_key",
        category: ErrorCategory::ForeignKeyViolation,
        pattern: r"foreign key constraint fails \(.*?FOREIGN KEY \(`(?P<key>[^`]+)`\) REFERENCES `(?P<table>[^`]+)`",
        key_groups: &["key"],
        render: |c| format!(
            "Hint: Foreign key constraint failed for '{}'. Ensure target {} exists before step execution.",
            first_column(get(c, &["key"])),
            singular(get(c, &["table"]))
        ),
    },
    Rule {
        name: "sqlite_foreign_key",
        category: ErrorCategory::ForeignKeyViolation,
        pattern: r"FOREIGN KEY constraint failed",
        key_groups: &[],
        render: |_| "Hint: Foreign key constraint failed. Ensure every referenced row exists before step execution.".into(),
    },
    Rule {
        name: "pg_unique",
        category: ErrorCategory::UniqueViolation,
        pattern: r"Key \((?P<key>[^)]+)\)=\((?P<value>[^)]*)\) already exists",
        key_groups: &["key"],
        render: |c| format!(
            "Hint: Unique constraint violated for '{}' (value '{}' already exists). Use a new value or update the existing row.",
            first_column(get(c, &["key"])),
            get(c, &["value"])
        ),
    },
    Rule {
        name: "mysql_duplicate",
        category: ErrorCategory::UniqueViolation,
        pattern: r"Duplicate entry '(?P<value>[^']*)' for key '(?:[^'.]*\.)?(?P<key>[^']+)'",
        key_groups: &["key"],
        render: |c| format!(
            "Hint: Unique constraint violated for '{}' (value '{}' already exists). Use a new value or update the existing row.",
            get(c, &["key"]),
            get(c, &["value"])
        ),
    },
    Rule {
        name: "sqlite_unique",
        category: ErrorCategory::UniqueViolation,
        pattern: r"UNIQUE constraint failed: (?:\w+\.)?(?P<key>\w+)",
        key_groups: &["key"],
        render: |c| format!(
            "Hint: Unique constraint violated for '{}'. Use a new value or update the existing row.",
            get(c, &["key"])
        ),
    },
    Rule {
        name: "not_null",
        category: ErrorCategory::NotNullViolation,
        pattern: r#"null value in column "(?P<key>[^"]+)"(?: of relation "[^"]+")? violates not-null constraint|Column '(?P<key2>[^']+)' cannot be null|NOT NULL constraint failed: (?:\w+\.)?(?P<key3>\w+)"#,
        key_groups: &["key", "key2", "key3"],
        render: |c| {
            let key = get(c, &["key", "key2", "key3"]);
            format!("Hint: Column '{key}' must not be null. Provide a value for '{key}'.")
        },
    },
    Rule {
        name: "missing_column",
        category: ErrorCategory::MissingSchemaObject,
        pattern: r#"column "(?P<key>[^"]+)"(?: of relation "[^"]+")? does not exist|no such column: (?:\w+\.)?(?P<key2>\w+)|Unknown column '(?:[^'.]*\.)?(?P<key3>[^']+)'"#,
        key_groups: &["key", "key2", "key3"],
        render: |c| format!(
            "Hint: Column '{}' does not exist. Check the schema and use an existing column.",
            get(c, &["key", "key2", "key3"])
        ),
    },
    Rule {
        name: "missing_table",
        category: ErrorCategory::MissingSchemaObject,
        pattern: r#"relation "(?P<table>[^"]+)" does not exist|no such table: (?P<table2>[\w.]+)|Table '(?:[^'.]*\.)?(?P<table3>[^']+)' doesn't exist"#,
        key_groups: &[],
        render: |c| format!(
            "Hint: Table '{}' does not exist. Create it first or use an existing table.",
            get(c, &["table", "table2", "table3"])
        ),
    },
    Rule {
        name: "record_not_found",
        category: ErrorCategory::MissingRecord,
        pattern: r#"no row with (?P<key>\w+)=(?P<value>\S+) in table "(?P<table>[^"]+)""#,
        key_groups: &["key"],
        render: |c| {
            let key = get(c, &["key"]);
            format!(
                "Hint: No {} with {key}={} exists. Create it in an earlier step or use an existing {key}.",
                singular(get(c, &["table"])),
                get(c, &["value"])
            )
        },
    },
    // --- Payload / language runtime errors --------------------------------
    Rule {
        name: "missing_field",
        category: ErrorCategory::MissingArgument,
        pattern: r"missing field `(?P<key>[^`]+)`|(?P<key2>\w+)\s*\n\s*Field required|'(?P<key3>[^']+)' is a required property",
        key_groups: &["key", "key2", "key3"],
        render: |c| {
            let key = get(c, &["key", "key2", "key3"]);
            format!("Hint: Required field '{key}' is missing from the arguments. Provide '{key}' explicitly.")
        },
    },
    Rule {
        name: "python_key_error",
        category: ErrorCategory::MissingKey,
        pattern: r#"KeyError: ['"](?P<key>[^'"]+)['"]"#,
        key_groups: &["key"],
        render: |c| {
            let key = get(c, &["key"]);
            format!("Hint: Missing key '{key}' in input payload. Verify the upstream step output provides '{key}'.")
        },
    },
    Rule {
        name: "python_missing_argument",
        category: ErrorCategory::MissingArgument,
        pattern: r"(?P<func>\w+)\(\) missing \d+ required (?:positional |keyword-only )?arguments?: '(?P<key>[^']+)'",
        key_groups: &["key"],
        render: |c| {
            let key = get(c, &["key"]);
            format!("Hint: {}() requires argument '{key}'. Pass '{key}' in the tool arguments.", get(c, &["func"]))
        },
    },
    Rule {
        name: "null_attribute",
        category: ErrorCategory::NullReference,
        pattern: r"'NoneType' object has no attribute '(?P<key>\w+)'|Cannot read propert(?:y|ies) of (?:undefined|null) \(reading '(?P<key2>[^']+)'\)",
        key_groups: &["key", "key2"],
        render: |c| format!(
            "Hint: Accessed '{}' on a null value. Ensure the upstream step returns a non-null object.",
            get(c, &["key", "key2"])
        ),
    },
    Rule {
        name: "java_npe",
        category: ErrorCategory::NullReference,
        // Leftmost-first alternation: capture the helpful-NPE detail as an
        // optional suffix so the exception prefix cannot shadow it.
        pattern: r#"java\.lang\.NullPointerException(?:: Cannot invoke "(?P<method>[^"]+)" because "(?P<key>[^"]+)" is null)?|Cannot invoke "(?P<method2>[^"]+)" because "(?P<key2>[^"]+)" is null"#,
        key_groups: &["key", "key2"],
        render: |c| match pick(c, &["key", "key2"]) {
            Some(key) => format!(
                "Hint: '{key}' is null when calling {}. Initialize '{key}' before this step.",
                get(c, &["method", "method2"])
            ),
            None => "Hint: Null reference encountered. Validate that all required inputs are non-null.".into(),
        },
    },
    Rule {
        name: "invalid_integer",
        category: ErrorCategory::InvalidValue,
        pattern: r#"invalid literal for int\(\) with base \d+: '(?P<value>[^']*)'|NumberFormatException: For input string: "(?P<value2>[^"]*)""#,
        key_groups: &[],
        render: |c| format!(
            "Hint: '{}' is not a valid integer. Pass a numeric value.",
            get(c, &["value", "value2"])
        ),
    },
    Rule {
        name: "type_mismatch",
        category: ErrorCategory::InvalidValue,
        pattern: r"invalid type: (?P<found>[^,\n]+), expected (?P<expected>[^\n]+?)(?: at line \d+ column \d+)?$",
        key_groups: &[],
        render: |c| format!(
            "Hint: Wrong argument type: got {}, expected {}.",
            get(c, &["found"]),
            get(c, &["expected"])
        ),
    },
    // --- Filesystem --------------------------------------------------------
    Rule {
        name: "path_escape",
        category: ErrorCategory::PermissionDenied,
        pattern: r"path '(?P<path>[^']*)' escapes the sandbox",
        key_groups: &[],
        render: |c| format!(
            "Hint: Path '{}' is outside the sandbox. Use a relative path without '..'.",
            get(c, &["path"])
        ),
    },
    Rule {
        name: "file_not_found",
        category: ErrorCategory::FileNotFound,
        pattern: r"No such file or directory(?: \(os error 2\))?(?:: '(?P<path>[^']+)')?|ENOENT: no such file or directory, \w+ '(?P<path2>[^']+)'|FileNotFoundException: (?P<path3>\S+)",
        key_groups: &[],
        render: |c| match pick(c, &["path", "path2", "path3"]) {
            Some(path) => format!("Hint: File '{path}' does not exist. Create it in an earlier step or correct the path."),
            None => "Hint: File does not exist. Create it in an earlier step or correct the path.".into(),
        },
    },
    Rule {
        name: "permission_denied",
        category: ErrorCategory::PermissionDenied,
        pattern: r"Permission denied(?: \(os error 13\))?(?:: '(?P<path>[^']+)')?|EACCES: permission denied, \w+ '(?P<path2>[^']+)'",
        key_groups: &[],
        render: |c| match pick(c, &["path", "path2"]) {
            Some(path) => format!("Hint: Permission denied for '{path}'. Use a path inside the writable sandbox."),
            None => "Hint: Permission denied. Use a resource the agent is allowed to modify.".into(),
        },
    },
    // --- Network / HTTP ----------------------------------------------------
    Rule {
        name: "rate_limited",
        category: ErrorCategory::RateLimited,
        pattern: r"(?i)\b(?:too many requests|rate[- ]limit(?:ed)?)\b|\b(?:HTTP(?:/\d(?:\.\d)?)?|status(?: code)?:?)\s*429\b",
        key_groups: &[],
        render: |_| "Hint: Rate limited by upstream API. Reduce request frequency or batch the calls.".into(),
    },
    Rule {
        name: "http_auth",
        category: ErrorCategory::Authentication,
        pattern: const_format_http!(r"(?P<status>401|403)\b"),
        key_groups: &[],
        render: |c| format!(
            "Hint: Upstream rejected credentials (HTTP {}). Use an authorized endpoint or valid credentials.",
            get(c, &["status"])
        ),
    },
    Rule {
        name: "http_not_found",
        category: ErrorCategory::HttpClientError,
        pattern: const_format_http!(r"404\b"),
        key_groups: &[],
        render: |_| "Hint: Upstream resource not found (HTTP 404). Verify the URL or resource identifier.".into(),
    },
    Rule {
        name: "http_client_error",
        category: ErrorCategory::HttpClientError,
        pattern: const_format_http!(r"(?P<status>4\d\d)\b"),
        key_groups: &[],
        render: |c| format!(
            "Hint: Upstream rejected the request (HTTP {}). Fix the request parameters.",
            get(c, &["status"])
        ),
    },
    Rule {
        name: "http_server_error",
        category: ErrorCategory::HttpServerError,
        pattern: const_format_http!(r"(?P<status>5\d\d)\b"),
        key_groups: &[],
        render: |c| format!(
            "Hint: Upstream service failed (HTTP {}). Retry later or use an alternative service.",
            get(c, &["status"])
        ),
    },
    Rule {
        name: "timeout",
        category: ErrorCategory::Timeout,
        pattern: r"(?i)\b(?:timed out|timeout|deadline exceeded)\b",
        key_groups: &[],
        render: |_| "Hint: Operation timed out. Reduce the work done in this step or split it into smaller steps.".into(),
    },
    Rule {
        name: "connection_failure",
        category: ErrorCategory::ConnectionFailure,
        pattern: r"(?i)\b(?:connection refused|connection reset|econnrefused|econnreset|could not connect|failed to connect)\b",
        key_groups: &[],
        render: |_| "Hint: Dependency is unreachable. Verify the host/port or use an available service.".into(),
    },
    Rule {
        name: "malformed_json",
        category: ErrorCategory::MalformedJson,
        pattern: r"(?i)JSONDecodeError|invalid arguments JSON|Unexpected token .{0,20} in JSON|expected value at line \d+ column \d+|EOF while parsing",
        key_groups: &[],
        render: |_| "Hint: Malformed JSON. Emit a single valid JSON object with double-quoted keys.".into(),
    },
];


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn helpers() {
        assert_eq!(first_column("(tenant_id, user_id)"), "tenant_id");
        assert_eq!(first_column("user_id"), "user_id");
        assert_eq!(singular("users"), "user");
        assert_eq!(singular("public.companies"), "company");
        assert_eq!(singular("addresses"), "address");
        assert_eq!(singular("status"), "statu"); // documented naive behavior
        assert_eq!(singular("class"), "class");
    }

    #[test]
    fn rule_names_are_unique() {
        let mut names: Vec<_> = RULES.iter().map(|r| r.name).collect();
        names.sort_unstable();
        let before = names.len();
        names.dedup();
        assert_eq!(before, names.len());
    }
}
