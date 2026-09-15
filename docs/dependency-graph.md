---
title: Dependency graph
description: How AgentTx links step outputs to later inputs and finds the step that actually caused a failure.
---

AgentTx keeps a directed graph for each transaction. Nodes are steps. An edge `P → C` labeled `(source_key, target_key)` means step `C` consumed, under input key `target_key`, a value that step `P` produced under output key `source_key`.

The graph is built on `petgraph::graph::DiGraph`.

## How edges are discovered

### Explicit edges

Every `${steps.N.path}` reference creates an explicit edge. It's the most reliable signal, so use references whenever a step consumes an earlier output.

```json
{ "customer_id": "${steps.2.id}" }
```

This records `2 → current` with source key `id` and target key `customer_id`.

### Implicit edges

For inputs without references, AgentTx links an input value to the **most recent** earlier output with an **equal scalar value** when either:

- the key names match (`user_id` ← `user_id`), or
- the input key ends in `_<output key>` (`user_id` ← `id`).

Booleans, nulls and empty strings never create implicit edges; they are too common to imply data flow.

## Finding the root cause

When step `K` fails with a hint naming `error_key`, `find_root_cause(K, error_key)` walks backwards:

1. From `K`, follow incoming edges whose target key matches the error key (case-insensitive).
2. At each producer, keep following upstream with the edge's source key. Renames like `id` → `customer_id` are handled.
3. If a producer passed a value through — its output under the key equals one of its own inputs — the walk continues through that input as well.
4. Return the **earliest** producer reached.

```text
step 2  create_user        output { id: "u-7" }
step 5  lookup_owner       input  { user: ${steps.2.id} }   output { owner_id: "u-7" }
step 9  insert_order       input  { user_id: ${steps.5.owner_id} }  ✕ FK violation on user_id

find_root_cause(9, "user_id") = 2
```

## Rewinds

Rewinding to step `T` removes step `T` and every later node and edge. `DiGraph` invalidates indices on removal, so the graph is rebuilt from the retained nodes in O(V + E). Re-executed steps add fresh nodes and edges.

## Tips

- Put the values your tools validate at the top level of their arguments, so error keys and input keys line up.
- Use references for anything that crosses more than one or two steps.
- Tools that transform values (e.g. normalize an id) should return the new value under the key the next step will use.
