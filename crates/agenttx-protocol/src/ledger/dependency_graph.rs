//! DAG of data dependencies between transaction steps.
//!
//! Nodes are steps; an edge `P -> C` labeled `(source_key, target_key)` means
//! step `C` consumed, under input key `target_key`, a value that step `P`
//! produced under output key `source_key`.
//!
//! Edges come from two sources:
//!
//! * **Explicit** — `${steps.P.path}` references in the arguments, resolved by
//!   the executor. Authoritative.
//! * **Implicit** — an input leaf whose scalar value equals an output leaf of
//!   an earlier step, where the key names match exactly or the input key ends
//!   in `_<output key>` (e.g. input `user_id` ← output `id`). The most recent
//!   producer wins.
//!
//! When step `K` fails with an error about `error_key`,
//! [`DependencyGraph::find_root_cause`] walks edges carrying that key upstream
//! (following renames) and returns the earliest producer: the step whose
//! output must change for `K` to succeed.

use std::collections::{HashMap, HashSet};

use petgraph::Direction;
use petgraph::graph::{DiGraph, NodeIndex};
use petgraph::visit::EdgeRef;
use serde_json::Value;

/// How a dependency was discovered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DependencyKind {
    Explicit,
    Implicit,
}

/// Edge payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dependency {
    /// Producer step.
    pub from_step: u32,
    /// Key name in the producer's output.
    pub source_key: String,
    /// Key name in the consumer's input.
    pub target_key: String,
    pub kind: DependencyKind,
}

#[derive(Debug, Clone)]
struct StepNode {
    step_id: u32,
    tool: String,
    /// Scalar input leaves `(key, value)` after reference resolution.
    inputs: Vec<(String, Value)>,
    /// Scalar output leaves `(key, value)`.
    outputs: Vec<(String, Value)>,
}

#[derive(Debug, Clone)]
struct EdgeLabel {
    source_key: String,
    target_key: String,
    kind: DependencyKind,
}

/// Step dependency DAG backed by `petgraph::graph::DiGraph`.
#[derive(Debug, Default, Clone)]
pub struct DependencyGraph {
    graph: DiGraph<StepNode, EdgeLabel>,
    nodes: HashMap<u32, NodeIndex>,
}

impl DependencyGraph {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.graph.node_count()
    }

    pub fn is_empty(&self) -> bool {
        self.graph.node_count() == 0
    }

    pub fn contains(&self, step_id: u32) -> bool {
        self.nodes.contains_key(&step_id)
    }

    /// Adds `step_id` with its (resolved) `input` and explicit dependencies,
    /// inferring implicit ones. Returns every dependency recorded.
    ///
    /// If the step already exists (re-execution after a rewind that did not
    /// truncate it) it and all later steps are replaced.
    pub fn add_step(
        &mut self,
        step_id: u32,
        tool: &str,
        input: &Value,
        explicit: &[Dependency],
    ) -> Vec<Dependency> {
        if self.contains(step_id) {
            self.truncate_from(step_id);
        }
        let node = self.graph.add_node(StepNode {
            step_id,
            tool: tool.to_owned(),
            inputs: scalar_leaves(input),
            outputs: Vec::new(),
        });
        self.nodes.insert(step_id, node);

        let mut recorded = Vec::new();
        let explicit_targets: HashSet<&str> =
            explicit.iter().map(|d| d.target_key.as_str()).collect();

        for dep in explicit {
            if let Some(&from) = self.nodes.get(&dep.from_step)
                && dep.from_step < step_id
            {
                self.graph.add_edge(
                    from,
                    node,
                    EdgeLabel {
                        source_key: dep.source_key.clone(),
                        target_key: dep.target_key.clone(),
                        kind: DependencyKind::Explicit,
                    },
                );
                recorded.push(Dependency {
                    kind: DependencyKind::Explicit,
                    ..dep.clone()
                });
            }
        }

        let mut producers: Vec<&StepNode> = self
            .nodes
            .iter()
            .filter(|(id, _)| **id < step_id)
            .map(|(_, idx)| &self.graph[*idx])
            .collect();
        producers.sort_by_key(|p| std::cmp::Reverse(p.step_id)); // most recent first

        let mut implicit = Vec::new();
        let mut seen = HashSet::new();
        for (in_key, in_value) in scalar_leaves(input) {
            if explicit_targets.contains(in_key.as_str()) || !is_linkable(&in_value) {
                continue;
            }
            let producer = producers.iter().find_map(|p| {
                p.outputs
                    .iter()
                    .find(|(out_key, out_value)| {
                        *out_value == in_value && keys_link(&in_key, out_key)
                    })
                    .map(|(out_key, _)| (p.step_id, out_key.clone()))
            });
            if let Some((from_step, source_key)) = producer
                && seen.insert((from_step, source_key.clone(), in_key.clone()))
            {
                implicit.push(Dependency {
                    from_step,
                    source_key,
                    target_key: in_key,
                    kind: DependencyKind::Implicit,
                });
            }
        }
        for dep in implicit {
            let from = self.nodes[&dep.from_step];
            self.graph.add_edge(
                from,
                node,
                EdgeLabel {
                    source_key: dep.source_key.clone(),
                    target_key: dep.target_key.clone(),
                    kind: DependencyKind::Implicit,
                },
            );
            recorded.push(dep);
        }
        recorded
    }

    /// Records the output of a successful step so later steps can link to it.
    pub fn record_output(&mut self, step_id: u32, output: &Value) {
        if let Some(&idx) = self.nodes.get(&step_id) {
            self.graph[idx].outputs = scalar_leaves(output);
        }
    }

    /// Returns the earliest upstream step responsible for `error_key` in the
    /// failure of `failed_step_id`, or `None` if no dependency carries it.
    ///
    /// Key comparison is ASCII case-insensitive. Renames are followed both on
    /// edges (output `id` consumed as `user_id`) and through tools: a producer
    /// whose output value under the key equals one of its own inputs is
    /// treated as passing that input through.
    pub fn find_root_cause(&self, failed_step_id: u32, error_key: &str) -> Option<u32> {
        let start = *self.nodes.get(&failed_step_id)?;
        let mut frontier = vec![(start, error_key.to_owned())];
        let mut visited: HashSet<(NodeIndex, String)> = HashSet::new();
        let mut root: Option<u32> = None;

        while let Some((node, key)) = frontier.pop() {
            let carried = self.carried_input_keys(node, &key);
            for edge in self.graph.edges_directed(node, Direction::Incoming) {
                let label = edge.weight();
                if !carried.contains(&label.target_key.to_ascii_lowercase()) {
                    continue;
                }
                let producer = edge.source();
                let next = (producer, label.source_key.to_ascii_lowercase());
                if visited.insert(next.clone()) {
                    let step = self.graph[producer].step_id;
                    root = Some(root.map_or(step, |r| r.min(step)));
                    frontier.push(next);
                }
            }
        }
        root
    }

    /// Lower-cased input keys through which `key` may have entered `node`:
    /// the key itself plus inputs whose value equals the node's output for it.
    fn carried_input_keys(&self, node: NodeIndex, key: &str) -> HashSet<String> {
        let step = &self.graph[node];
        let produced: Vec<&Value> = step
            .outputs
            .iter()
            .filter(|(k, _)| k.eq_ignore_ascii_case(key))
            .map(|(_, v)| v)
            .collect();
        std::iter::once(key.to_ascii_lowercase())
            .chain(
                step.inputs
                    .iter()
                    .filter(|(_, v)| is_linkable(v) && produced.contains(&v))
                    .map(|(k, _)| k.to_ascii_lowercase()),
            )
            .collect()
    }

    /// Direct dependencies of `step_id` (producers it consumed from).
    pub fn dependencies_of(&self, step_id: u32) -> Vec<Dependency> {
        let Some(&idx) = self.nodes.get(&step_id) else {
            return Vec::new();
        };
        let mut deps: Vec<Dependency> = self
            .graph
            .edges_directed(idx, Direction::Incoming)
            .map(|e| Dependency {
                from_step: self.graph[e.source()].step_id,
                source_key: e.weight().source_key.clone(),
                target_key: e.weight().target_key.clone(),
                kind: e.weight().kind,
            })
            .collect();
        deps.sort_by(|a, b| (a.from_step, &a.target_key).cmp(&(b.from_step, &b.target_key)));
        deps
    }

    /// Steps that directly consumed an output of `step_id`.
    pub fn dependents_of(&self, step_id: u32) -> Vec<u32> {
        let Some(&idx) = self.nodes.get(&step_id) else {
            return Vec::new();
        };
        let mut out: Vec<u32> = self
            .graph
            .neighbors_directed(idx, Direction::Outgoing)
            .map(|n| self.graph[n].step_id)
            .collect();
        out.sort_unstable();
        out.dedup();
        out
    }

    /// Tool name recorded for `step_id`.
    pub fn tool_of(&self, step_id: u32) -> Option<&str> {
        self.nodes
            .get(&step_id)
            .map(|idx| self.graph[*idx].tool.as_str())
    }

    /// Removes `step_id` and every later step (rewind to before `step_id`).
    ///
    /// `DiGraph::remove_node` invalidates indices, so the graph is rebuilt
    /// from retained nodes: O(V + E).
    pub fn truncate_from(&mut self, step_id: u32) {
        if self.nodes.keys().all(|id| *id < step_id) {
            return;
        }
        let mut rebuilt: DiGraph<StepNode, EdgeLabel> = DiGraph::new();
        let mut remap = HashMap::new();
        let mut kept: Vec<NodeIndex> = self
            .graph
            .node_indices()
            .filter(|idx| self.graph[*idx].step_id < step_id)
            .collect();
        kept.sort_by_key(|idx| self.graph[*idx].step_id);
        for old in kept {
            let new = rebuilt.add_node(self.graph[old].clone());
            remap.insert(old, new);
        }
        for edge in self.graph.edge_references() {
            if let (Some(&s), Some(&t)) = (remap.get(&edge.source()), remap.get(&edge.target())) {
                rebuilt.add_edge(s, t, edge.weight().clone());
            }
        }
        self.nodes = rebuilt
            .node_indices()
            .map(|idx| (rebuilt[idx].step_id, idx))
            .collect();
        self.graph = rebuilt;
    }

    pub fn clear(&mut self) {
        self.graph.clear();
        self.nodes.clear();
    }
}

/// Flattens JSON into `(key, scalar)` pairs. Array elements inherit the key of
/// their containing field; top-level scalars use the empty key.
fn scalar_leaves(value: &Value) -> Vec<(String, Value)> {
    fn walk(key: &str, value: &Value, out: &mut Vec<(String, Value)>) {
        match value {
            Value::Object(map) => {
                for (k, v) in map {
                    walk(k, v, out);
                }
            }
            Value::Array(items) => {
                for item in items {
                    walk(key, item, out);
                }
            }
            scalar => out.push((key.to_owned(), scalar.clone())),
        }
    }
    let mut out = Vec::new();
    walk("", value, &mut out);
    out
}

/// Booleans, nulls and empty strings are too common to imply data flow.
fn is_linkable(value: &Value) -> bool {
    match value {
        Value::String(s) => !s.is_empty(),
        Value::Number(_) => true,
        _ => false,
    }
}

fn keys_link(input_key: &str, output_key: &str) -> bool {
    if input_key.is_empty() || output_key.is_empty() {
        return false;
    }
    if input_key.eq_ignore_ascii_case(output_key) {
        return true;
    }
    let input = input_key.to_ascii_lowercase();
    let output = output_key.to_ascii_lowercase();
    input.ends_with(&format!("_{output}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn explicit(from_step: u32, source: &str, target: &str) -> Dependency {
        Dependency {
            from_step,
            source_key: source.into(),
            target_key: target.into(),
            kind: DependencyKind::Explicit,
        }
    }

    #[test]
    fn infers_implicit_dependency_by_key_and_value() {
        let mut g = DependencyGraph::new();
        g.add_step(
            1,
            "record.insert",
            &json!({"table": "users", "id": "u-101"}),
            &[],
        );
        g.record_output(1, &json!({"table": "users", "id": "u-101"}));
        let deps = g.add_step(
            2,
            "record.insert",
            &json!({"table": "orders", "id": "o-1", "fields": {"user_id": "u-101"}}),
            &[],
        );
        assert!(deps.contains(&Dependency {
            from_step: 1,
            source_key: "id".into(),
            target_key: "user_id".into(),
            kind: DependencyKind::Implicit,
        }));
        assert_eq!(g.dependents_of(1), vec![2]);
    }

    #[test]
    fn value_mismatch_creates_no_edge() {
        let mut g = DependencyGraph::new();
        g.add_step(1, "t", &json!({}), &[]);
        g.record_output(1, &json!({"user_id": 7}));
        let deps = g.add_step(2, "t", &json!({"user_id": 8, "flag": true}), &[]);
        assert!(deps.is_empty());
    }

    #[test]
    fn root_cause_follows_renames_to_earliest_producer() {
        let mut g = DependencyGraph::new();
        g.add_step(1, "create_user", &json!({}), &[]);
        g.record_output(1, &json!({"id": 101}));
        g.add_step(
            2,
            "lookup",
            &json!({"user": 101}),
            &[explicit(1, "id", "user")],
        );
        g.record_output(2, &json!({"owner_id": 101}));
        g.add_step(3, "unrelated", &json!({"x": "y"}), &[]);
        g.record_output(3, &json!({"z": 1}));
        g.add_step(
            4,
            "insert_order",
            &json!({"user_id": 101}),
            &[explicit(2, "owner_id", "user_id")],
        );

        assert_eq!(g.find_root_cause(4, "user_id"), Some(1));
        assert_eq!(g.find_root_cause(4, "USER_ID"), Some(1));
        assert_eq!(g.find_root_cause(4, "other"), None);
        assert_eq!(g.find_root_cause(99, "user_id"), None);
    }

    #[test]
    fn explicit_edges_suppress_duplicate_implicit_edges() {
        let mut g = DependencyGraph::new();
        g.add_step(1, "a", &json!({}), &[]);
        g.record_output(1, &json!({"id": 5}));
        let deps = g.add_step(2, "b", &json!({"id": 5}), &[explicit(1, "id", "id")]);
        assert_eq!(deps.len(), 1);
        assert_eq!(deps[0].kind, DependencyKind::Explicit);
    }

    #[test]
    fn explicit_edges_to_unknown_or_future_steps_are_ignored() {
        let mut g = DependencyGraph::new();
        let deps = g.add_step(1, "a", &json!({}), &[explicit(5, "id", "id")]);
        assert!(deps.is_empty());
    }

    #[test]
    fn truncate_removes_later_steps_and_edges() {
        let mut g = DependencyGraph::new();
        for step in 1..=4 {
            let input = if step == 1 {
                json!({})
            } else {
                json!({"v": step - 1})
            };
            g.add_step(step, "t", &input, &[]);
            g.record_output(step, &json!({"v": step}));
        }
        assert_eq!(g.find_root_cause(4, "v"), Some(1));
        g.truncate_from(3);
        assert_eq!(g.len(), 2);
        assert!(!g.contains(3));
        assert_eq!(g.dependents_of(1), vec![2]);
        assert_eq!(g.tool_of(2), Some("t"));
        assert_eq!(g.find_root_cause(2, "v"), Some(1));

        // Re-adding a step replaces it and everything after it.
        g.add_step(2, "t2", &json!({}), &[]);
        assert_eq!(g.tool_of(2), Some("t2"));
        assert!(g.dependencies_of(2).is_empty());
    }

    #[test]
    fn handles_cycles_in_key_walk_without_looping() {
        // A pathological pass-through chain where keys repeat.
        let mut g = DependencyGraph::new();
        g.add_step(1, "t", &json!({}), &[]);
        g.record_output(1, &json!({"k": "v"}));
        g.add_step(2, "t", &json!({"k": "v"}), &[]);
        g.record_output(2, &json!({"k": "v"}));
        g.add_step(3, "t", &json!({"k": "v"}), &[]);
        // Implicit linking picks the most recent producer (2), which itself
        // consumed `k` from 1.
        assert_eq!(g.find_root_cause(3, "k"), Some(1));
        g.clear();
        assert!(g.is_empty());
    }
}
