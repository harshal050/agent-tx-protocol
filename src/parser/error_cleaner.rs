//! Turns raw tool errors and stack traces into one-line "Clean Hints".
//!
//! Matching is purely deterministic (a compiled `RegexSet` plus rule
//! renderers), costs microseconds and involves no LLM call. Inputs are bounded
//! to [`MAX_INPUT_BYTES`] (head and tail kept) so pathological traces cannot
//! inflate latency.

use std::borrow::Cow;
use std::sync::LazyLock;

use regex::{Regex, RegexSet};

use super::ErrorCategory;
use super::rules::{RULES, Rule, first_column, pick};

/// Longest input examined; longer inputs keep their head and tail.
pub const MAX_INPUT_BYTES: usize = 64 * 1024;
/// Longest hint emitted, in characters.
pub const MAX_HINT_CHARS: usize = 240;

/// A deterministic, single-line, actionable summary of a failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CleanHint {
    /// Always starts with `Hint: ` and never contains a newline.
    pub text: String,
    pub category: ErrorCategory,
    /// Name of the matching rule; `None` for the generic fallback.
    pub rule: Option<&'static str>,
    /// Offending key (e.g. a column), used to locate root-cause steps.
    pub key: Option<String>,
}

/// Compiled rule engine.
#[derive(Debug)]
pub struct ErrorCleaner {
    set: RegexSet,
    regexes: Vec<Regex>,
    rules: &'static [Rule],
}

static EXCEPTION_LINE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(?:[\w$]+\.)*[A-Z][\w$]*(?:Error|Exception|Fault)\b").expect("static regex")
});
static QUALIFIED_EXCEPTION: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(?:[a-z_$][\w$]*\.)+([A-Z][\w$]*(?:Error|Exception|Fault))").expect("static regex")
});
static FRAME_NUMBER: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(?:#?\d+[:\s]|\d+: )").expect("static regex"));

impl ErrorCleaner {
    /// Compiles the built-in [`RULES`].
    pub fn new() -> Result<Self, regex::Error> {
        Self::with_rules(RULES)
    }

    /// Compiles a custom rule table (first matching rule wins).
    pub fn with_rules(rules: &'static [Rule]) -> Result<Self, regex::Error> {
        let set = RegexSet::new(rules.iter().map(|r| r.pattern))?;
        let regexes = rules
            .iter()
            .map(|r| Regex::new(r.pattern))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self { set, regexes, rules })
    }

    /// Process-wide instance with the built-in rules.
    ///
    /// The built-in patterns are compile-checked by this module's tests.
    pub fn global() -> &'static ErrorCleaner {
        static GLOBAL: LazyLock<ErrorCleaner> =
            LazyLock::new(|| ErrorCleaner::new().expect("built-in rules compile"));
        &GLOBAL
    }

    /// Produces a hint for `raw`. Never fails; unmatched input falls back to
    /// the most salient line of the trace.
    pub fn clean(&self, raw: &str) -> CleanHint {
        let text = bounded(raw);
        if let Some(index) = self.set.matches(&text).iter().next() {
            let rule = &self.rules[index];
            if let Some(caps) = self.regexes[index].captures(&text) {
                let key = pick(&caps, rule.key_groups).map(first_column);
                return CleanHint {
                    text: one_line((rule.render)(&caps)),
                    category: rule.category,
                    rule: Some(rule.name),
                    key: key.filter(|k| !k.is_empty()),
                };
            }
        }
        let line = salient_line(&text).unwrap_or_else(|| "unknown error".to_owned());
        CleanHint {
            text: one_line(format!(
                "Hint: Step failed with \"{line}\". Change the arguments so this error cannot recur."
            )),
            category: ErrorCategory::Unknown,
            rule: None,
            key: None,
        }
    }
}

/// Keeps the first quarter and last three quarters of oversized input.
fn bounded(raw: &str) -> Cow<'_, str> {
    if raw.len() <= MAX_INPUT_BYTES {
        return Cow::Borrowed(raw);
    }
    let head_end = floor_boundary(raw, MAX_INPUT_BYTES / 4);
    let tail_start = ceil_boundary(raw, raw.len() - (MAX_INPUT_BYTES / 4) * 3);
    Cow::Owned(format!("{}\n...\n{}", &raw[..head_end], &raw[tail_start..]))
}

fn floor_boundary(s: &str, mut i: usize) -> usize {
    while !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}

fn ceil_boundary(s: &str, mut i: usize) -> usize {
    while !s.is_char_boundary(i) {
        i += 1;
    }
    i
}

fn is_frame(line: &str) -> bool {
    const PREFIXES: &[&str] = &[
        "at ",
        "File \"",
        "Traceback",
        "...",
        "During handling",
        "The above exception",
        "stack backtrace",
        "note: run with",
    ];
    PREFIXES.iter().any(|p| line.starts_with(p))
        || line.chars().all(|c| matches!(c, '^' | '~' | ' '))
        || FRAME_NUMBER.is_match(line)
}

/// Picks the line most likely to describe the root failure:
/// the last `Caused by:` (Java), else the last exception line (Python), else
/// the first non-frame line.
fn salient_line(text: &str) -> Option<String> {
    let lines: Vec<&str> = text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    let chosen = lines
        .iter()
        .rev()
        .find_map(|l| l.strip_prefix("Caused by:").map(str::trim))
        .or_else(|| {
            lines
                .iter()
                .rev()
                .copied()
                .find(|l| !is_frame(l) && EXCEPTION_LINE.is_match(l))
        })
        .or_else(|| lines.iter().copied().find(|l| !is_frame(l)))?;
    Some(QUALIFIED_EXCEPTION.replace(chosen, "$1").into_owned())
}

/// Collapses whitespace and truncates to [`MAX_HINT_CHARS`].
fn one_line(text: String) -> String {
    let collapsed = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.chars().count() <= MAX_HINT_CHARS {
        return collapsed;
    }
    let mut truncated: String = collapsed.chars().take(MAX_HINT_CHARS - 1).collect();
    truncated.push('…');
    truncated
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clean(raw: &str) -> CleanHint {
        ErrorCleaner::global().clean(raw)
    }

    #[test]
    fn all_builtin_rules_compile() {
        ErrorCleaner::new().expect("rules compile");
    }

    #[test]
    fn postgres_foreign_key_matches_spec_example() {
        let raw = r#"org.postgresql.util.PSQLException: ERROR: insert or update on table "orders" violates foreign key constraint "orders_user_id_fkey"
  Detail: Key (user_id)=(101) is not present in table "users".
	at org.postgresql.core.v3.QueryExecutorImpl.receiveErrorResponse(QueryExecutorImpl.java:2676)
	at org.postgresql.core.v3.QueryExecutorImpl.processResults(QueryExecutorImpl.java:2366)
	at com.zaxxer.hikari.pool.HikariProxyPreparedStatement.executeUpdate(HikariProxyPreparedStatement.java)"#;
        let hint = clean(raw);
        assert_eq!(
            hint.text,
            "Hint: Foreign key constraint failed for 'user_id'. Ensure target user exists before step execution."
        );
        assert_eq!(hint.category, ErrorCategory::ForeignKeyViolation);
        assert_eq!(hint.key.as_deref(), Some("user_id"));
        assert_eq!(hint.rule, Some("pg_foreign_key"));
    }

    #[test]
    fn composite_keys_use_first_column() {
        let hint = clean(r#"Key (tenant_id, user_id)=(1, 2) is not present in table "public.users""#);
        assert_eq!(hint.key.as_deref(), Some("tenant_id"));
        assert!(hint.text.contains("target user exists"));
    }

    #[test]
    fn python_traceback_key_error() {
        let raw = "Traceback (most recent call last):\n  File \"agent.py\", line 12, in <module>\n    run(payload)\n  File \"agent.py\", line 8, in run\n    uid = payload['user_id']\nKeyError: 'user_id'\n";
        let hint = clean(raw);
        assert_eq!(hint.category, ErrorCategory::MissingKey);
        assert_eq!(hint.key.as_deref(), Some("user_id"));
        assert!(hint.text.starts_with("Hint: Missing key 'user_id'"));
    }

    #[test]
    fn table_of_representative_errors() {
        let cases: &[(&str, &str, Option<&str>)] = &[
            (r#"ERROR: duplicate key value violates unique constraint "users_email_key" DETAIL: Key (email)=(a@b.c) already exists."#, "pg_unique", Some("email")),
            ("Cannot add or update a child row: a foreign key constraint fails (`shop`.`orders`, CONSTRAINT `fk` FOREIGN KEY (`customer_id`) REFERENCES `customers` (`id`))", "mysql_foreign_key", Some("customer_id")),
            ("sqlite3.IntegrityError: FOREIGN KEY constraint failed", "sqlite_foreign_key", None),
            ("java.sql.SQLIntegrityConstraintViolationException: Duplicate entry 'bob' for key 'users.username'", "mysql_duplicate", Some("username")),
            ("UNIQUE constraint failed: users.email", "sqlite_unique", Some("email")),
            (r#"null value in column "name" of relation "users" violates not-null constraint"#, "not_null", Some("name")),
            ("Column 'price' cannot be null", "not_null", Some("price")),
            (r#"column "emial" does not exist"#, "missing_column", Some("emial")),
            ("sqlite3.OperationalError: no such table: invoices", "missing_table", None),
            (r#"no row with id=u-9 in table "users""#, "record_not_found", Some("id")),
            ("Error(\"missing field `amount`\", line: 1, column: 20)", "missing_field", Some("amount")),
            ("1 validation error for Order\nquantity\n  Field required [type=missing]", "missing_field", Some("quantity")),
            ("TypeError: create_order() missing 1 required positional argument: 'sku'", "python_missing_argument", Some("sku")),
            ("AttributeError: 'NoneType' object has no attribute 'email'", "null_attribute", Some("email")),
            ("TypeError: Cannot read properties of undefined (reading 'id')", "null_attribute", Some("id")),
            (r#"java.lang.NullPointerException: Cannot invoke "String.length()" because "name" is null"#, "java_npe", Some("name")),
            ("ValueError: invalid literal for int() with base 10: 'abc'", "invalid_integer", None),
            ("invalid type: string \"x\", expected u32 at line 1 column 9", "type_mismatch", None),
            ("path '../etc/passwd' escapes the sandbox", "path_escape", None),
            ("FileNotFoundError: [Errno 2] No such file or directory: 'report.csv'", "file_not_found", None),
            ("Error: EACCES: permission denied, open '/root/x'", "permission_denied", None),
            ("HTTP 429 Too Many Requests", "rate_limited", None),
            ("request failed with status code: 403", "http_auth", None),
            ("GET https://api.example.com/v1/items returned 404", "http_not_found", None),
            ("HTTP/1.1 422 Unprocessable Entity", "http_client_error", None),
            ("upstream responded with status 503", "http_server_error", None),
            ("tokio: deadline exceeded while waiting for response", "timeout", None),
            ("psycopg2.OperationalError: could not connect to server: Connection refused", "connection_failure", None),
            ("json.decoder.JSONDecodeError: Expecting value: line 1 column 1 (char 0)", "malformed_json", None),
            ("unknown tool 'sql.run'; registered tools: kv.get, kv.put", "unknown_tool", None),
            ("unresolved reference ${steps.2.user_id}: key 'user_id' not present in output of step 2", "unresolved_reference", Some("user_id")),
        ];
        for (raw, rule, key) in cases {
            let hint = clean(raw);
            assert_eq!(hint.rule, Some(*rule), "input: {raw}\nhint: {hint:?}");
            assert_eq!(hint.key.as_deref(), *key, "input: {raw}");
            assert!(hint.text.starts_with("Hint: "), "{hint:?}");
            assert!(!hint.text.contains('\n'));
        }
    }

    #[test]
    fn fallback_prefers_java_root_cause() {
        let raw = "com.acme.ServiceException: step failed\n\tat com.acme.Svc.run(Svc.java:10)\nCaused by: com.acme.QuotaExceededException: monthly quota reached\n\tat com.acme.Quota.check(Quota.java:5)";
        let hint = clean(raw);
        assert_eq!(hint.rule, None);
        assert_eq!(hint.category, ErrorCategory::Unknown);
        assert!(
            hint.text.contains("QuotaExceededException: monthly quota reached"),
            "{}",
            hint.text
        );
        assert!(!hint.text.contains("com.acme.Quota"));
    }

    #[test]
    fn fallback_uses_last_python_exception_line() {
        let raw = "Traceback (most recent call last):\n  File \"x.py\", line 1, in <module>\n    boom()\ncustom.errors.InventoryError: sku 42 is discontinued";
        let hint = clean(raw);
        assert!(hint.text.contains("InventoryError: sku 42 is discontinued"), "{}", hint.text);
    }

    #[test]
    fn hints_are_bounded_and_single_line() {
        let raw = format!("{}\nsomething odd happened {}", "x".repeat(200_000), "y ".repeat(500));
        let hint = clean(&raw);
        assert!(hint.text.chars().count() <= MAX_HINT_CHARS);
        assert!(!hint.text.contains('\n'));
    }

    #[test]
    fn bounded_window_respects_char_boundaries() {
        let raw = "é".repeat(MAX_INPUT_BYTES);
        let window = bounded(&raw);
        assert!(window.len() < raw.len());
        assert!(window.contains("\n...\n"));
    }

    #[test]
    fn empty_input_is_handled() {
        let hint = clean("   \n  ");
        assert_eq!(hint.category, ErrorCategory::Unknown);
        assert!(hint.text.contains("unknown error"));
    }
}
