use indexmap::IndexMap;
/// Graph-based AST for the rhai pseudo-DSL.
///
/// Two statement forms are supported:
///   - Pipeline step:    `fn name() -> target`
///   - Conditional:      `if expr -> target`   where expr is e.g. `confidence > 0.8`
///   - Match arm:        `match expr => Arm -> target`
///
/// The syntax is stable — richer identity and placement semantics are encoded
/// in the parser output, not in a second incompatible syntax.
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub enum NodeKind {
    Step,
    Decision,
    Match,
}

/// Semantic role inferred from node label keywords.
/// Drives icon selection, color, and layout lift in both Mermaid and
/// isometric renderers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SemanticRole {
    Ingest,
    Validate,
    Classify,
    Review,
    Reconcile,
    Commit,
    Decision,
    Step,
}

impl SemanticRole {
    pub fn infer(label: &str, kind: &NodeKind) -> Self {
        if matches!(kind, NodeKind::Decision | NodeKind::Match) {
            return SemanticRole::Decision;
        }
        let lower = label.to_lowercase();
        if lower.contains("ingest")
            || lower.contains("load")
            || lower.contains("parse")
            || lower.contains("extract")
            || lower.contains("source")
            || lower.contains("input")
        {
            return SemanticRole::Ingest;
        }
        if lower.contains("validate")
            || lower.contains("verify")
            || lower.contains("check")
            || lower.contains("guard")
            || lower.contains("audit")
            || lower.contains("rule")
        {
            return SemanticRole::Validate;
        }
        if lower.contains("classify")
            || lower.contains("label")
            || lower.contains("tag")
            || lower.contains("map")
            || lower.contains("route")
        {
            return SemanticRole::Classify;
        }
        if lower.contains("review")
            || lower.contains("approve")
            || lower.contains("manual")
            || lower.contains("operator")
            || lower.contains("human")
        {
            return SemanticRole::Review;
        }
        if lower.contains("reconcile")
            || lower.contains("match")
            || lower.contains("balance")
            || lower.contains("ledger")
        {
            return SemanticRole::Reconcile;
        }
        if lower.contains("commit")
            || lower.contains("publish")
            || lower.contains("export")
            || lower.contains("write")
            || lower.contains("persist")
            || lower.contains("done")
            || lower.contains("finish")
        {
            return SemanticRole::Commit;
        }
        SemanticRole::Step
    }

    #[allow(dead_code)]
    pub fn key(&self) -> &'static str {
        match self {
            SemanticRole::Ingest => "ingest",
            SemanticRole::Validate => "validate",
            SemanticRole::Classify => "classify",
            SemanticRole::Review => "review",
            SemanticRole::Reconcile => "reconcile",
            SemanticRole::Commit => "commit",
            SemanticRole::Decision => "decision",
            SemanticRole::Step => "step",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Node {
    pub id: String,
    /// Stable identity key — survives cosmetic label changes.
    /// Defaults to the same as `id` but can be set explicitly for
    /// identity-stable reflow across source edits.
    #[allow(dead_code)]
    pub identity_key: String,
    pub label: String,
    pub kind: NodeKind,
    /// Semantic role inferred from label keywords.
    #[allow(dead_code)]
    pub role: SemanticRole,
    /// For match arms: declaration order index within the match group.
    #[allow(dead_code)]
    pub arm_index: Option<usize>,
    /// Whether this node is a default/fallback arm (`_` or `else`).
    #[allow(dead_code)]
    pub is_default: bool,
}

#[derive(Debug, Clone)]
pub struct Edge {
    pub from: String,
    pub to: String,
    pub label: Option<String>,
    /// For match arms: declaration order index.
    #[allow(dead_code)]
    pub arm_index: Option<usize>,
    /// Whether this edge represents a default/fallback path.
    pub is_default: bool,
}

#[derive(Debug, Default)]
pub struct Graph {
    pub order: Vec<String>,
    pub nodes: IndexMap<String, Node>,
    pub edges: Vec<Edge>,
}

impl Graph {
    pub fn add_node(&mut self, id: String, label: String, kind: NodeKind) {
        if !self.nodes.contains_key(&id) {
            self.order.push(id.clone());
            let role = SemanticRole::infer(&label, &kind);
            self.nodes.insert(
                id.clone(),
                Node {
                    id: id.clone(),
                    identity_key: id,
                    label,
                    kind,
                    role,
                    arm_index: None,
                    is_default: false,
                },
            );
        }
    }

    pub fn add_node_rich(
        &mut self,
        id: String,
        identity_key: String,
        label: String,
        kind: NodeKind,
        arm_index: Option<usize>,
        is_default: bool,
    ) {
        if !self.nodes.contains_key(&id) {
            self.order.push(id.clone());
            let role = SemanticRole::infer(&label, &kind);
            self.nodes.insert(
                id.clone(),
                Node {
                    id,
                    identity_key,
                    label,
                    kind,
                    role,
                    arm_index,
                    is_default,
                },
            );
        }
    }

    pub fn add_edge(&mut self, from: String, to: String, label: Option<String>) {
        self.edges.push(Edge {
            from,
            to,
            label,
            arm_index: None,
            is_default: false,
        });
    }

    pub fn add_edge_rich(
        &mut self,
        from: String,
        to: String,
        label: Option<String>,
        arm_index: Option<usize>,
        is_default: bool,
    ) {
        self.edges.push(Edge {
            from,
            to,
            label,
            arm_index,
            is_default,
        });
    }
}

// ---------------------------------------------------------------------------
// Sanitization
// ---------------------------------------------------------------------------

pub fn sanitize_id(raw: &str) -> String {
    raw.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Parsing
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct Conditional {
    lhs: String,
    op: String,
    rhs: String,
    target: String,
}

#[derive(Debug, Clone)]
struct MatchArm {
    expr: String,
    arm: String,
    target: String,
}

/// Parse the rhai pseudo-DSL source into a `Graph`.
///
/// Returns an empty `Graph` when the source contains no parseable statements
/// (empty or comment-only input). Malformed lines are silently skipped so a
/// partial parse always succeeds.
pub fn parse(source: &str) -> Graph {
    let mut pipeline_edges: Vec<(String, String)> = Vec::new();
    let mut conditionals: Vec<Conditional> = Vec::new();
    let mut match_arms: Vec<MatchArm> = Vec::new();

    for raw_line in source.lines() {
        let line = match raw_line.find("//") {
            Some(pos) => &raw_line[..pos],
            None => raw_line,
        }
        .trim();

        if line.is_empty() {
            continue;
        }

        if let Some(rest) = line.strip_prefix("fn ") {
            if let Some((name_part, target_part)) = rest.split_once("->") {
                let name = name_part.trim().trim_end_matches("()").trim().to_string();
                let target = target_part.trim().to_string();
                if !name.is_empty() && !target.is_empty() {
                    pipeline_edges.push((name, target));
                }
            }
        } else if let Some(rest) = line.strip_prefix("if ") {
            if let Some((expr_part, target_part)) = rest.split_once("->") {
                let expr = expr_part.trim().to_string();
                let target = target_part.trim().to_string();
                if !expr.is_empty() && !target.is_empty() {
                    if let Some(cond) = parse_condition(&expr, &target) {
                        conditionals.push(cond);
                    } else {
                        let cond_id = sanitize_id(&expr);
                        let target_id = sanitize_id(&target);
                        conditionals.push(Conditional {
                            lhs: cond_id,
                            op: String::new(),
                            rhs: String::new(),
                            target: target_id,
                        });
                    }
                }
            }
        } else if let Some(rest) = line.strip_prefix("match ") {
            if let Some((expr_part, arm_target_part)) = rest.split_once("=>") {
                if let Some((arm_part, target_part)) = arm_target_part.split_once("->") {
                    let expr = expr_part.trim().to_string();
                    let arm = arm_part.trim().to_string();
                    let target = target_part.trim().to_string();
                    if !expr.is_empty() && !arm.is_empty() && !target.is_empty() {
                        match_arms.push(MatchArm { expr, arm, target });
                    }
                }
            }
        }
    }

    build_graph(pipeline_edges, conditionals, match_arms)
}

fn parse_condition(expr: &str, target: &str) -> Option<Conditional> {
    let operators = [">=", "<=", "!=", ">", "<", "=="];
    for op in &operators {
        if let Some(pos) = expr.find(op) {
            let lhs = expr[..pos].trim().to_string();
            let rhs = expr[pos + op.len()..].trim().to_string();
            if !lhs.is_empty() && !rhs.is_empty() {
                return Some(Conditional {
                    lhs,
                    op: op.to_string(),
                    rhs,
                    target: target.to_string(),
                });
            }
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Graph builder
// ---------------------------------------------------------------------------

fn build_graph(
    pipeline_edges: Vec<(String, String)>,
    conditionals: Vec<Conditional>,
    match_arms: Vec<MatchArm>,
) -> Graph {
    let mut graph = Graph::default();

    for (name, target) in &pipeline_edges {
        let name_id = sanitize_id(name);
        let target_id = sanitize_id(target);
        graph.add_node(name_id.clone(), name.clone(), NodeKind::Step);
        graph.add_node(target_id.clone(), target.clone(), NodeKind::Step);
        graph.add_edge(name_id, target_id, None);
    }

    let mut gt_groups: HashMap<String, Vec<(f64, String)>> = HashMap::new();
    let mut lt_groups: HashMap<String, Vec<(f64, String)>> = HashMap::new();
    let mut plain_conds: Vec<Conditional> = Vec::new();

    for cond in &conditionals {
        if cond.op == ">" {
            if let Ok(val) = cond.rhs.parse::<f64>() {
                gt_groups
                    .entry(cond.lhs.clone())
                    .or_default()
                    .push((val, cond.target.clone()));
                continue;
            }
        }
        if cond.op == "<" {
            if let Ok(val) = cond.rhs.parse::<f64>() {
                lt_groups
                    .entry(cond.lhs.clone())
                    .or_default()
                    .push((val, cond.target.clone()));
                continue;
            }
        }
        plain_conds.push(cond.clone());
    }

    for (lhs, mut thresholds) in gt_groups {
        thresholds.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        emit_threshold_chain(&mut graph, &lhs, ">", &thresholds);
    }

    for (lhs, mut thresholds) in lt_groups {
        thresholds.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
        emit_threshold_chain(&mut graph, &lhs, "<", &thresholds);
    }

    for cond in &plain_conds {
        if cond.op.is_empty() {
            let cond_id = cond.lhs.clone();
            let target_id = sanitize_id(&cond.target);
            graph.add_node(cond_id.clone(), cond.lhs.clone(), NodeKind::Decision);
            graph.add_node(target_id.clone(), cond.target.clone(), NodeKind::Step);
            graph.add_edge(cond_id, target_id, None);
        } else {
            let expr_label = format!("{} {} {}", cond.lhs, cond.op, cond.rhs);
            let cond_id = sanitize_id(&expr_label);
            let target_id = sanitize_id(&cond.target);
            graph.add_node(cond_id.clone(), expr_label, NodeKind::Decision);
            graph.add_node(target_id.clone(), cond.target.clone(), NodeKind::Step);
            graph.add_edge(cond_id, target_id, Some("true".to_string()));
        }
    }

    emit_match_groups(&mut graph, &match_arms);

    graph
}

fn emit_match_groups(graph: &mut Graph, match_arms: &[MatchArm]) {
    let mut groups: IndexMap<String, Vec<(String, String)>> = IndexMap::new();

    for arm in match_arms {
        groups
            .entry(arm.expr.clone())
            .or_default()
            .push((arm.arm.clone(), arm.target.clone()));
    }

    for (expr, arms) in groups {
        let node_id = format!("match_{}", sanitize_id(&expr));
        let label = format!("match {}", expr);
        graph.add_node(node_id.clone(), label, NodeKind::Match);

        for (arm_index, (arm_label, target)) in arms.iter().enumerate() {
            let target_id = sanitize_id(target);
            let is_default = is_default_arm(arm_label);
            graph.add_node_rich(
                target_id.clone(),
                target_id.clone(),
                target.clone(),
                NodeKind::Step,
                Some(arm_index),
                is_default,
            );
            graph.add_edge_rich(
                node_id.clone(),
                target_id,
                Some(arm_label.clone()),
                Some(arm_index),
                is_default,
            );
        }
    }
}

fn is_default_arm(arm_label: &str) -> bool {
    let trimmed = arm_label.trim();
    matches!(trimmed, "_" | "else" | "otherwise" | "default")
}

fn emit_threshold_chain(graph: &mut Graph, lhs: &str, op: &str, thresholds: &[(f64, String)]) {
    let node_ids: Vec<String> = thresholds
        .iter()
        .map(|(val, _)| sanitize_id(&format!("{}_{}_{}", lhs, op_to_word(op), val_to_safe(val))))
        .collect();

    for (i, (val, target)) in thresholds.iter().enumerate() {
        let node_id = &node_ids[i];
        let label = format!("{} {} {}", lhs, op, val);
        graph.add_node(node_id.clone(), label, NodeKind::Decision);

        let target_id = sanitize_id(target);
        graph.add_node(target_id.clone(), target.clone(), NodeKind::Step);
        graph.add_edge(node_id.clone(), target_id, Some("true".to_string()));

        if i + 1 < thresholds.len() {
            let next_id = node_ids[i + 1].clone();
            graph.add_edge(node_id.clone(), next_id, Some("false".to_string()));
        }
    }
}

fn op_to_word(op: &str) -> &str {
    match op {
        ">" => "gt",
        "<" => "lt",
        ">=" => "gte",
        "<=" => "lte",
        "==" => "eq",
        "!=" => "ne",
        _ => "op",
    }
}

fn val_to_safe(val: &f64) -> String {
    format!("{}", val).replace('.', "_")
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn find_node_by_label<'a>(graph: &'a Graph, label: &str) -> Option<&'a Node> {
        graph.nodes.values().find(|n| n.label == label)
    }

    #[test]
    fn test_pipeline_chain() {
        let src = r#"
            fn ingest() -> classify
            fn classify() -> reconcile
            fn reconcile() -> export
        "#;
        let g = parse(src);
        assert!(g.nodes.contains_key("ingest"));
        assert!(g.nodes.contains_key("classify"));
        assert!(g.nodes.contains_key("reconcile"));
        assert!(g.nodes.contains_key("export"));
        assert_eq!(g.edges.len(), 3);
        assert_eq!(g.edges[0].from, "ingest");
        assert_eq!(g.edges[0].to, "classify");
        assert!(g.edges[0].label.is_none());
    }

    #[test]
    fn test_conditional_branch() {
        let src = r#"
            if confidence > 0.8 -> commit
        "#;
        let g = parse(src);
        let decision = g.nodes.values().find(|n| n.kind == NodeKind::Decision);
        assert!(decision.is_some());
        let commit = g.nodes.values().find(|n| n.label == "commit");
        assert!(commit.is_some());
        assert_eq!(g.edges.len(), 1);
        assert_eq!(g.edges[0].label.as_deref(), Some("true"));
    }

    #[test]
    fn test_threshold_chain_ordering() {
        let src = r#"
            if confidence > 0.5 -> reconcile
            if confidence > 0.8 -> commit
        "#;
        let g = parse(src);
        let decision_nodes: Vec<&Node> = g
            .nodes
            .values()
            .filter(|n| n.kind == NodeKind::Decision)
            .collect();
        assert_eq!(decision_nodes.len(), 2);

        let false_edge = g.edges.iter().find(|e| e.label.as_deref() == Some("false"));
        assert!(false_edge.is_some(), "expected a false-chain edge");

        let fe = false_edge.unwrap();
        assert!(
            fe.from.contains("0_8"),
            "false edge from should reference 0.8 threshold"
        );
        assert!(
            fe.to.contains("0_5"),
            "false edge to should reference 0.5 threshold"
        );
    }

    #[test]
    fn test_deduplication() {
        let src = r#"
            fn ingest() -> classify
            fn ingest() -> classify
        "#;
        let g = parse(src);
        assert_eq!(g.nodes.len(), 2);
        assert_eq!(g.edges.len(), 2);
    }

    #[test]
    fn test_sanitized_node_ids() {
        let src = "fn parse docs() -> build index\n";
        let g = parse(src);
        assert!(g.nodes.contains_key("parse_docs"));
        assert!(g.nodes.contains_key("build_index"));
    }

    #[test]
    fn test_operator_coverage() {
        let ops = [
            ("if score >= 0.9 -> high", ">="),
            ("if score <= 0.1 -> low", "<="),
            ("if status == approved -> commit", "=="),
            ("if status != rejected -> continue", "!="),
        ];
        for (src, op) in &ops {
            let g = parse(src);
            let decision = g.nodes.values().find(|n| n.kind == NodeKind::Decision);
            assert!(decision.is_some(), "no decision node for op {}", op);
            let node = decision.unwrap();
            assert!(
                node.label.contains(op),
                "label '{}' does not contain op '{}'",
                node.label,
                op
            );
        }
    }

    #[test]
    fn test_comment_stripping() {
        let src = r#"
            fn ingest() -> classify // this is a comment
            // full-line comment
            fn classify() -> done
        "#;
        let g = parse(src);
        assert_eq!(g.nodes.len(), 3);
    }

    #[test]
    fn test_match_group_builds_single_match_node_with_labeled_arms() {
        let src = r#"
            match result.disposition => Disposition::Unrecoverable -> halt_pipeline
            match result.disposition => Disposition::Recoverable -> repair_and_retry
            match result.disposition => Disposition::Advisory -> record_note
        "#;
        let g = parse(src);

        let match_nodes: Vec<&Node> = g
            .nodes
            .values()
            .filter(|n| n.kind == NodeKind::Match)
            .collect();
        assert_eq!(match_nodes.len(), 1);
        assert_eq!(match_nodes[0].label, "match result.disposition");
        assert_eq!(g.edges.len(), 3);
        assert_eq!(
            g.edges[0].label.as_deref(),
            Some("Disposition::Unrecoverable")
        );
        assert_eq!(
            g.edges[1].label.as_deref(),
            Some("Disposition::Recoverable")
        );
        assert_eq!(g.edges[2].label.as_deref(), Some("Disposition::Advisory"));
    }

    #[test]
    fn test_empty_input() {
        let g = parse("// only comments\n\n");
        assert!(g.nodes.is_empty());
        assert!(g.edges.is_empty());
    }

    #[test]
    fn test_find_node_by_label_helper() {
        let src = "fn ingest() -> classify\n";
        let g = parse(src);
        assert!(find_node_by_label(&g, "ingest").is_some());
        assert!(find_node_by_label(&g, "nonexistent").is_none());
    }

    #[test]
    fn test_semantic_role_inference() {
        let src = r#"
            fn ingest_pdf() -> validate_rows
            fn validate_rows() -> classify_transactions
            fn classify_transactions() -> reconcile_xero
            fn reconcile_xero() -> review_flags
            fn review_flags() -> commit_workbook
        "#;
        let g = parse(src);

        assert_eq!(g.nodes["ingest_pdf"].role, SemanticRole::Ingest);
        assert_eq!(g.nodes["validate_rows"].role, SemanticRole::Validate);
        assert_eq!(
            g.nodes["classify_transactions"].role,
            SemanticRole::Classify
        );
        assert_eq!(g.nodes["reconcile_xero"].role, SemanticRole::Reconcile);
        assert_eq!(g.nodes["review_flags"].role, SemanticRole::Review);
        assert_eq!(g.nodes["commit_workbook"].role, SemanticRole::Commit);
    }

    #[test]
    fn test_match_arm_ordering_preserved() {
        let src = r#"
            match result.disposition => Disposition::Unrecoverable -> halt
            match result.disposition => Disposition::Recoverable -> retry
            match result.disposition => Disposition::Advisory -> log
        "#;
        let g = parse(src);

        let edges: Vec<&Edge> = g.edges.iter().collect();
        assert_eq!(edges.len(), 3);
        assert_eq!(edges[0].arm_index, Some(0));
        assert_eq!(edges[1].arm_index, Some(1));
        assert_eq!(edges[2].arm_index, Some(2));
    }

    #[test]
    fn test_default_arm_detection() {
        let src = r#"
            match result.disposition => Disposition::Unrecoverable -> halt
            match result.disposition => _ -> fallback
        "#;
        let g = parse(src);

        assert_eq!(g.edges.len(), 2);
        assert!(!g.edges[0].is_default);
        assert!(g.edges[1].is_default);
    }

    #[test]
    fn test_identity_key_stability() {
        let src = "fn ingest() -> classify\n";
        let g = parse(src);

        let ingest = &g.nodes["ingest"];
        assert_eq!(ingest.id, "ingest");
        assert_eq!(ingest.identity_key, "ingest");
        assert_eq!(ingest.label, "ingest");
    }

    // -----------------------------------------------------------------------
    // Malformed / negative input tests — parser must not panic and should
    // gracefully skip or partially parse malformed lines.
    // -----------------------------------------------------------------------

    #[test]
    fn test_malformed_misspelled_fn() {
        let g = parse("fnn ingest() -> classify\n");
        assert!(g.nodes.is_empty());
        assert!(g.edges.is_empty());
    }

    #[test]
    fn test_malformed_missing_target_arrow() {
        let g = parse("fn ingest()\n");
        assert!(g.nodes.is_empty());
        assert!(g.edges.is_empty());
    }

    #[test]
    fn test_malformed_if_missing_target() {
        let g = parse("if confidence > 0.5 ->\n");
        assert!(g.nodes.is_empty());
        assert!(g.edges.is_empty());
    }

    #[test]
    fn test_malformed_double_arrow() {
        // split_once("->") splits on first occurrence, so target is "-> classify"
        // which is non-empty — parser produces a node named "-> classify".
        // Still must not panic and should produce a valid Graph.
        // split_once("->") captures "-> classify" as the target.
        // sanitize_id turns "-> classify" into "___classify".
        let g = parse("fn ingest() -> -> classify\n");
        assert!(g.nodes.contains_key("ingest"));
        assert!(g.nodes.contains_key("___classify"));
        assert_eq!(g.nodes.len(), 2);
        assert_eq!(g.edges.len(), 1);
    }

    #[test]
    fn test_malformed_empty_target_name() {
        let g = parse("fn ingest() -> \n");
        assert!(g.nodes.is_empty());
        assert!(g.edges.is_empty());
    }

    #[test]
    fn test_malformed_match_missing_arm() {
        let src = "match result -> Unrecoverable -> \n";
        let g = parse(src);
        // match needs expr, arm, AND target all non-empty — empty target skips
        assert!(g.nodes.is_empty());
        assert!(g.edges.is_empty());
    }

    #[test]
    fn test_malformed_very_long_label() {
        let long_name = "x".repeat(1000);
        let src = format!("fn {}() -> done\n", long_name);
        let g = parse(&src);
        // Parser may or may not include the node, but must not panic.
        // The sanitized id will be valid since 'x' is alphanumeric.
        assert!(g.nodes.is_empty() || g.nodes.len() == 2);
        assert!(g.edges.is_empty() || g.edges.len() == 1);
        if let Some(node) = g.nodes.get(&long_name) {
            assert_eq!(node.id, long_name);
            assert_eq!(node.identity_key, long_name);
        }
    }
}
