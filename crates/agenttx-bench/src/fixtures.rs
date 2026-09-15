//! Benchmark tools and a corpus of realistic raw errors.

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use async_trait::async_trait;
use serde_json::{Value, json};

use agenttx::engine::{Tool, ToolContext, ToolError, ToolRegistry, builtin_tools};

/// Built-in tools plus the simulation's service-backed tools.
pub fn registry(sandbox: &Path) -> ToolRegistry {
    let registry = ToolRegistry::new();
    builtin_tools::register_builtin_tools(&registry, sandbox);
    let record_insert = registry
        .get("record.insert")
        .expect("record.insert is a built-in tool");
    registry.register(Arc::new(InsertInvoice {
        inner: record_insert,
    }));
    registry.register(Arc::new(Flaky {
        failures_left: AtomicU32::new(1),
    }));
    registry.register(Arc::new(Broken));
    registry
}

/// `record.insert` exposed the way a Python tool service would surface it:
/// constraint errors arrive wrapped in a psycopg traceback.
struct InsertInvoice {
    inner: Arc<dyn Tool>,
}

#[async_trait]
impl Tool for InsertInvoice {
    fn name(&self) -> &str {
        "bench.insert_invoice"
    }

    async fn execute(&self, ctx: &ToolContext, args: Value) -> Result<Value, ToolError> {
        match self.inner.execute(ctx, args).await {
            Err(ToolError::Failed(message)) => Err(ToolError::Failed(psycopg_trace(&message))),
            other => other,
        }
    }
}

fn psycopg_trace(message: &str) -> String {
    let mut lines = message
        .lines()
        .map(|l| l.trim_start_matches("ERROR: ").trim());
    let first = lines.next().unwrap_or_default();
    let detail = lines.collect::<Vec<_>>().join("\n");
    format!(
        r#"Traceback (most recent call last):
  File "/srv/agent-tools/billing/invoices.py", line 88, in insert_invoice
    await cur.execute(INSERT_INVOICE, params)
  File "/usr/local/lib/python3.12/site-packages/psycopg/cursor_async.py", line 97, in execute
    raise ex.with_traceback(None)
psycopg.errors.IntegrityError: {first}
{detail}

The above exception was the direct cause of the following exception:

Traceback (most recent call last):
  File "/srv/agent-tools/server.py", line 212, in dispatch
    result = await registry.call(request.tool, request.arguments)
  File "/srv/agent-tools/registry.py", line 64, in call
    return await tool(**arguments)
  File "/srv/agent-tools/billing/invoices.py", line 94, in insert_invoice
    raise ToolExecutionError("insert_invoice failed") from exc
agent_tools.errors.ToolExecutionError: insert_invoice failed"#
    )
}

/// Fails with a gRPC deadline trace `failures_left` times, then succeeds.
struct Flaky {
    failures_left: AtomicU32,
}

#[async_trait]
impl Tool for Flaky {
    fn name(&self) -> &str {
        "bench.flaky"
    }

    async fn execute(&self, _ctx: &ToolContext, _args: Value) -> Result<Value, ToolError> {
        let fail = self
            .failures_left
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |n| n.checked_sub(1))
            .is_ok();
        if fail {
            return Err(ToolError::failed(TIMEOUT_TRACE));
        }
        Ok(json!({ "charged": true, "amount": 1200 }))
    }
}

/// Always fails with an unrecognized domain error (fallback hint path).
struct Broken;

#[async_trait]
impl Tool for Broken {
    fn name(&self) -> &str {
        "bench.broken"
    }

    async fn execute(&self, _ctx: &ToolContext, _args: Value) -> Result<Value, ToolError> {
        Err(ToolError::failed(LEDGER_MISMATCH_TRACE))
    }
}

const TIMEOUT_TRACE: &str = r#"Traceback (most recent call last):
  File "/srv/agent-tools/payments/charge.py", line 51, in charge_card
    response = await stub.Charge(request, timeout=5.0)
  File "/usr/local/lib/python3.12/site-packages/grpc/aio/_call.py", line 327, in __await__
    raise _create_rpc_error(
grpc.aio._call.AioRpcError: <AioRpcError of RPC that terminated with:
	status = StatusCode.DEADLINE_EXCEEDED
	details = "Deadline Exceeded"
	debug_error_string = "UNKNOWN:Error received from peer ipv4:10.0.4.17:443 {grpc_message:"Deadline Exceeded", grpc_status:4}"
>"#;

const LEDGER_MISMATCH_TRACE: &str = r#"Traceback (most recent call last):
  File "/srv/agent-tools/ledger/reconcile.py", line 133, in reconcile_invoice
    assert_totals_match(invoice, ledger_entries)
  File "/srv/agent-tools/ledger/checks.py", line 27, in assert_totals_match
    raise LedgerMismatchError(
ledger.errors.LedgerMismatchError: invoice total 1200.00 does not match ledger total 1180.00"#;

/// A raw error as an agent would receive it from a real tool backend.
pub struct ErrorFixture {
    pub id: &'static str,
    pub label: &'static str,
    pub runtime: &'static str,
    pub raw: &'static str,
}

pub static ERROR_FIXTURES: &[ErrorFixture] = &[
    ErrorFixture {
        id: "java-postgres-fk",
        label: "PostgreSQL foreign key (Spring/JDBC)",
        runtime: "Java",
        raw: r#"org.springframework.dao.DataIntegrityViolationException: could not execute statement; SQL [n/a]; constraint [orders_user_id_fkey]
	at org.springframework.orm.jpa.vendor.HibernateJpaDialect.convertHibernateAccessException(HibernateJpaDialect.java:276)
	at org.springframework.orm.jpa.vendor.HibernateJpaDialect.translateExceptionIfPossible(HibernateJpaDialect.java:233)
	at org.springframework.dao.support.PersistenceExceptionTranslationInterceptor.invoke(PersistenceExceptionTranslationInterceptor.java:137)
	at org.springframework.aop.framework.ReflectiveMethodInvocation.proceed(ReflectiveMethodInvocation.java:184)
	at com.acme.orders.OrderService$$SpringCGLIB$$0.createOrder(<generated>)
	at com.acme.agent.tools.CreateOrderTool.execute(CreateOrderTool.java:58)
	at com.acme.agent.runtime.ToolDispatcher.dispatch(ToolDispatcher.java:112)
Caused by: org.hibernate.exception.ConstraintViolationException: could not execute statement
	at org.hibernate.exception.internal.SQLStateConversionDelegate.convert(SQLStateConversionDelegate.java:95)
	at org.hibernate.engine.jdbc.spi.SqlExceptionHelper.convert(SqlExceptionHelper.java:56)
	at org.hibernate.engine.jdbc.internal.ResultSetReturnImpl.executeUpdate(ResultSetReturnImpl.java:197)
	at org.hibernate.persister.entity.AbstractEntityPersister.insert(AbstractEntityPersister.java:3375)
	... 42 more
Caused by: org.postgresql.util.PSQLException: ERROR: insert or update on table "orders" violates foreign key constraint "orders_user_id_fkey"
  Detail: Key (user_id)=(101) is not present in table "users".
	at org.postgresql.core.v3.QueryExecutorImpl.receiveErrorResponse(QueryExecutorImpl.java:2713)
	at org.postgresql.core.v3.QueryExecutorImpl.processResults(QueryExecutorImpl.java:2401)
	at org.postgresql.core.v3.QueryExecutorImpl.execute(QueryExecutorImpl.java:368)
	at org.postgresql.jdbc.PgStatement.executeInternal(PgStatement.java:498)
	at org.postgresql.jdbc.PgPreparedStatement.executeWithFlags(PgPreparedStatement.java:152)
	at com.zaxxer.hikari.pool.ProxyPreparedStatement.executeUpdate(ProxyPreparedStatement.java:61)
	at com.zaxxer.hikari.pool.HikariProxyPreparedStatement.executeUpdate(HikariProxyPreparedStatement.java)
	... 51 more"#,
    },
    ErrorFixture {
        id: "python-keyerror",
        label: "Missing key in tool payload",
        runtime: "Python",
        raw: r#"Traceback (most recent call last):
  File "/app/.venv/lib/python3.12/site-packages/langchain_core/tools/base.py", line 689, in run
    response = context.run(self._run, *tool_args, **tool_kwargs)
  File "/app/tools/crm.py", line 41, in _run
    return self.client.update_contact(payload)
  File "/app/clients/crm_client.py", line 118, in update_contact
    contact_id = payload["contact_id"]
                 ~~~~~~~^^^^^^^^^^^^^^
KeyError: 'contact_id'"#,
    },
    ErrorFixture {
        id: "node-undefined",
        label: "Property read on undefined",
        runtime: "Node.js",
        raw: r#"TypeError: Cannot read properties of undefined (reading 'id')
    at buildShipment (/srv/tools/shipping/src/shipment.ts:74:31)
    at ShippingTool.invoke (/srv/tools/shipping/src/tool.ts:39:18)
    at async ToolRouter.handle (/srv/tools/core/src/router.ts:122:20)
    at async /srv/tools/core/node_modules/@modelcontextprotocol/sdk/dist/esm/server/index.js:204:28
    at async Promise.all (index 0)"#,
    },
    ErrorFixture {
        id: "java-mysql-duplicate",
        label: "MySQL duplicate entry (JDBC)",
        runtime: "Java",
        raw: r#"java.sql.SQLIntegrityConstraintViolationException: Duplicate entry 'ada@example.com' for key 'users.email'
	at com.mysql.cj.jdbc.exceptions.SQLError.createSQLException(SQLError.java:118)
	at com.mysql.cj.jdbc.exceptions.SQLExceptionsMapping.translateException(SQLExceptionsMapping.java:122)
	at com.mysql.cj.jdbc.ClientPreparedStatement.executeInternal(ClientPreparedStatement.java:916)
	at com.mysql.cj.jdbc.ClientPreparedStatement.executeUpdateInternal(ClientPreparedStatement.java:1061)
	at com.mysql.cj.jdbc.ClientPreparedStatement.executeLargeUpdate(ClientPreparedStatement.java:1346)
	at com.zaxxer.hikari.pool.HikariProxyPreparedStatement.executeUpdate(HikariProxyPreparedStatement.java)
	at com.acme.users.UserRepository.insert(UserRepository.java:44)
	at com.acme.agent.tools.SignupTool.execute(SignupTool.java:31)"#,
    },
    ErrorFixture {
        id: "python-http-429",
        label: "Upstream rate limit",
        runtime: "Python",
        raw: r#"Traceback (most recent call last):
  File "/app/tools/search.py", line 63, in search_web
    response.raise_for_status()
  File "/app/.venv/lib/python3.12/site-packages/httpx/_models.py", line 829, in raise_for_status
    raise HTTPStatusError(message, request=request, response=self)
httpx.HTTPStatusError: Client error '429 Too Many Requests' for url 'https://api.search.example.com/v2/query?q=agent+transactions'
For more information check: https://developer.mozilla.org/en-US/docs/Web/HTTP/Status/429"#,
    },
    ErrorFixture {
        id: "go-connection-refused",
        label: "Database unreachable",
        runtime: "Go",
        raw: r#"inventory tool failed: reserve stock for sku "SKU-4412": failed to connect to `host=10.0.3.12 user=inventory database=inventory`: dial error (dial tcp 10.0.3.12:5432: connect: connection refused)
goroutine 187 [running]:
github.com/acme/agent-tools/inventory.(*Service).Reserve(0xc0001a2000, {0x1a3b2c0, 0xc000482000}, {0xc00012e0f0, 0x8}, 0x3)
	/src/inventory/service.go:92 +0x3c5
github.com/acme/agent-tools/rpc.(*ToolServer).Call(0xc000118480, {0x1a3b2c0, 0xc000482000}, 0xc0004a4000)
	/src/rpc/server.go:141 +0x2b1"#,
    },
    ErrorFixture {
        id: "rust-serde-missing-field",
        label: "Argument validation (serde)",
        runtime: "Rust",
        raw: r#"Error: failed to execute tool `create_refund`

Caused by:
    0: invalid tool arguments
    1: missing field `amount_cents` at line 1 column 48

Stack backtrace:
   0: anyhow::error::<impl anyhow::Error>::msg
   1: refund_tool::parse_args
             at ./src/lib.rs:88:18
   2: refund_tool::execute
             at ./src/lib.rs:41:20"#,
    },
    ErrorFixture {
        id: "java-unknown-domain",
        label: "Unrecognized domain error (fallback)",
        runtime: "Java",
        raw: r#"com.acme.billing.BillingException: failed to finalize invoice INV-2291
	at com.acme.billing.InvoiceService.finalizeInvoice(InvoiceService.java:210)
	at com.acme.agent.tools.FinalizeInvoiceTool.execute(FinalizeInvoiceTool.java:47)
	at com.acme.agent.runtime.ToolDispatcher.dispatch(ToolDispatcher.java:112)
Caused by: com.acme.billing.TaxJurisdictionException: no tax rule configured for region 'EU-HR' and product class 'digital-services'
	at com.acme.billing.tax.TaxEngine.resolve(TaxEngine.java:88)
	at com.acme.billing.InvoiceService.applyTaxes(InvoiceService.java:301)
	... 3 more"#,
    },
];
