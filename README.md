# AgentTx Protocol 🛡️⚡
> **Transactional Security & Micro-State Management Layer for Autonomous AI Agents**

**AgentTx Protocol** is an enterprise-grade, async Rust middleware engineered to sit directly between LLM runtimes (OpenAI, Claude, Cursor, LangGraph) and external execution tools (Model Context Protocol / MCP Servers, DBs, APIs). 

By bringing **ACID transaction guarantees** to AI Agent execution loops, AgentTx prevents context poisoning, isolates multi-agent states, and enables instant sub-3ms state rollbacks when untrusted data payloads or indirect prompt injections are detected.

---

### 🔑 Key Features

* **⚡ Zero-Latency Micro-State Engine (RocksDB + Memory Delta Store):** 
  Captures fast context diffs instead of full payload snapshots. If an agent step encounters untrusted output, AgentTx executes an instant rollback to Step $N-1$ without killing the active agent session.

* **🛡️ Real-Time Prompt Injection & Poison Shield:** 
  Native Rust policy engine that intercepts inbound/outbound tool payloads to filter out indirect prompt injections, malicious hidden prompts, and invisible Unicode triggers before they reach the LLM.

* **🧠 Semantic Entropy & Evals Engine (ONNX Runtime):** 
  Runs local, lightweight SLMs (MiniLM/Phi-3) directly inside Rust via ONNX C++ bindings to calculate reasoning divergence and context entropy in under 25ms, avoiding expensive external API calls.

* **🔄 ACID Transaction Guarantees:** 
  Provides Atomicity, Consistency, Isolation, and Durability across tool calls and long-term vector memory integrations (Zep, Mem0, Pinecone).

* **🔌 Native MCP Middleware:** 
  Zero-friction integration with Anthropic's Model Context Protocol (MCP) standard via standard JSON-RPC and Server-Sent Events (SSE) proxies.

---

### 🏗️ High-Level Architecture Flow