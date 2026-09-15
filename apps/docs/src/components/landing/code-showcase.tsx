import { Tabs } from "@agenttx/ui/interactive";

import { SectionHeading } from "@/components/section-heading";
import { CodeBlock, Markdown } from "@/components/markdown";
import { highlight } from "@/lib/content";

const SNIPPETS = [
  {
    id: "python",
    label: "Python",
    lang: "python",
    code: `r = client.ExecuteStep(pb.ExecuteStepRequest(
    transaction_id=tx, step_id=next_step,
    tool_name=call.tool, arguments_json=json.dumps(call.args),
))

if r.status == pb.STEP_STATUS_ROLLBACK_TRIGGERED:
    agent.rewind(r.rollback_to_step, reset_context=r.context_reset)
    system_prompt.constraints = list(r.constraints)   # one-line Clean Hints

next_step = r.next_step_id`,
  },
  {
    id: "typescript",
    label: "TypeScript",
    lang: "ts",
    code: `const r = await executeStep({
  transaction_id,
  step_id: nextStep,
  tool_name: "record.insert",
  arguments_json: JSON.stringify({ table: "invoices", fields: { customer_id: "\${steps.2.value}" } }),
});

if (r.status === "STEP_STATUS_ROLLBACK_TRIGGERED") {
  console.log(r.strategy, r.clean_hint); // DEPENDENCY_JUMP, "Hint: Foreign key constraint failed…"
}
nextStep = r.next_step_id;`,
  },
  {
    id: "rust",
    label: "Rust (embedded)",
    lang: "rust",
    code: `let tx = engine.begin("billing-agent", Default::default(), None).await?.tx_id;

let outcome = engine.execute_step(StepRequest {
    tx_id: tx.clone(),
    step_id: 1,
    tool_name: "fs.write".into(),
    arguments_json: r#"{"path":"drafts/inv-1.json","contents":"{}"}"#.into(),
    raw_context: String::new(),
}).await?;

engine.commit(&tx).await?; // publishes state, then dispatches staged emails`,
  },
  {
    id: "grpcurl",
    label: "grpcurl",
    lang: "bash",
    code: `grpcurl -plaintext -d '{"agent_id":"quickstart"}' \\
  127.0.0.1:50051 agenttx.v1.AgentTxService/BeginTransaction

grpcurl -plaintext -d @ 127.0.0.1:50051 agenttx.v1.AgentTxService/ExecuteStep <<EOF
{ "transaction_id": "$TX", "step_id": 1, "tool_name": "kv.put",
  "arguments_json": "{\\"key\\":\\"customer\\",\\"value\\":\\"c-1\\"}" }
EOF`,
  },
];

export async function CodeShowcase() {
  const highlighted = await Promise.all(SNIPPETS.map((snippet) => highlight(snippet.code, snippet.lang)));

  return (
    <section className="mx-auto grid max-w-7xl items-start gap-12 px-4 py-24 sm:px-6 sm:py-28 lg:grid-cols-[0.85fr_1.15fr]">
      <SectionHeading
        eyebrow="Integration"
        title="A dozen lines in the loop you already have."
        description="Send tool calls through ExecuteStep, always resume at next_step_id, and put the returned constraints in your prompt. Works from any language with gRPC."
      />
      <Tabs
        ariaLabel="Client examples"
        className="min-w-0 rounded-2xl border border-border bg-surface p-2"
        listClassName="px-1 pb-2"
        panelClassName="[&_.code-block]:my-0"
        items={SNIPPETS.map((snippet, index) => ({
          id: snippet.id,
          label: snippet.label,
          content: <Markdown key={snippet.id} tree={highlighted[index]!} />,
        }))}
      />
    </section>
  );
}

export { CodeBlock };
