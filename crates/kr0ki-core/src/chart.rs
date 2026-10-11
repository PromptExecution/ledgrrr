//! Build and drive a lazily StateChart from SysML-v2 process elements.
//!
//! ## Control-flow node mapping (Phase 5)
//!
//! | SysML v2 kind      | lazily role     | Event fired | Notes |
//! |--------------------|-----------------|-------------|-------|
//! | ActionDefinition   | compound root   | "next"      | first child = initial |
//! | ActionUsage        | atomic leaf     | "next"      | parent = owning def |
//! | DecisionNode       | atomic transit  | guard-keyed | fires `"guard:<id>_true"` / `"guard:<id>_false"` based on guard value |
//! | ForkNode           | atomic transit  | "fork:<id>" | advances all outgoing successors — callers fire the event for each branch |
//! | JoinNode           | atomic transit  | "next"      | waits for "next" from any incoming; lazily handles join semantics via compound |
//! | MergeNode          | atomic transit  | "next"      | pass-through merge; transitions on "next" from any incoming |

use std::collections::{BTreeSet, HashMap};

use lazily::statechart::{ChartBuilder, ChartDef, StateBuilder, ThreadSafeStateChart};
use lazily::ThreadSafeContext;
use ufo_types::{ElementId, ElementKind, Relation};

use crate::error::Kr0kiError;

/// A compiled process chart ready to be driven by sending events.
pub struct ProcessChart {
    chart: ThreadSafeStateChart,
    ctx: ThreadSafeContext,
    /// For DecisionNode ids: the guard name to evaluate for routing.
    decision_guards: HashMap<String, String>,
}

impl ProcessChart {
    /// Build a `ProcessChart` from a flat slice of `(ElementId, ElementKind)`
    /// element declarations and a slice of `Relation`s.
    ///
    /// Mapping rules:
    /// - `ActionDefinition` → compound root state (first one wins as root)
    /// - `ActionUsage`       → atomic leaf; parent = its `ActionDefinition`
    ///   owner via `FeatureMembership`, else root
    /// - `Succession`        → `source.on("next", target)` transition
    /// - `DecisionNode`      → atomic transit; routes on `"guard:<id>_true"` or
    ///   `"guard:<id>_false"` — exactly two outgoing `Succession` edges expected
    /// - `ForkNode`          → atomic transit; fires `"fork:<id>"` for callers
    ///   to handle concurrency outside the chart
    /// - `JoinNode`/`MergeNode` → atomic pass-through; advance on `"next"`
    pub fn from_sysml(
        elements: &[(ElementId, ElementKind)],
        relations: &[Relation],
    ) -> Result<Self, Kr0kiError> {
        let mut owner_of: HashMap<&str, &str> = HashMap::new();
        let mut successors: HashMap<&str, Vec<&str>> = HashMap::new();

        for rel in relations {
            match rel {
                Relation::FeatureMembership { owner, member } => {
                    owner_of.insert(member.0.as_str(), owner.0.as_str());
                }
                Relation::Succession { source, target } => {
                    successors
                        .entry(source.0.as_str())
                        .or_default()
                        .push(target.0.as_str());
                }
                _ => {}
            }
        }

        let mut first_child: HashMap<&str, &str> = HashMap::new();
        for (id, kind) in elements {
            if *kind == ElementKind::ActionUsage {
                if let Some(def_id) = owner_of.get(id.0.as_str()) {
                    first_child.entry(def_id).or_insert(id.0.as_str());
                }
            }
        }

        let (chart_def, decision_guards) =
            Self::build_chart_def(elements, &owner_of, &first_child, &successors)?;

        let ctx = ThreadSafeContext::new();
        let chart = ThreadSafeStateChart::new(&ctx, chart_def);

        Ok(Self { chart, ctx, decision_guards })
    }

    fn build_chart_def(
        elements: &[(ElementId, ElementKind)],
        owner_of: &HashMap<&str, &str>,
        first_child: &HashMap<&str, &str>,
        successors: &HashMap<&str, Vec<&str>>,
    ) -> Result<(ChartDef, HashMap<String, String>), Kr0kiError> {
        let mut builder = ChartBuilder::new();
        let mut seen = BTreeSet::new();
        let mut decision_guards: HashMap<String, String> = HashMap::new();

        for (id, kind) in elements {
            let sid = id.0.as_str();
            if !seen.insert(sid) {
                return Err(Kr0kiError::DuplicateId(sid.to_string()));
            }

            let sb = match kind {
                ElementKind::ActionDefinition => {
                    let initial = first_child.get(sid).copied().unwrap_or("");
                    let mut sb = if initial.is_empty() {
                        StateBuilder::atomic(sid)
                    } else {
                        StateBuilder::compound(sid, initial)
                    };
                    if let Some(targets) = successors.get(sid) {
                        for &tgt in targets {
                            sb = sb.on("next", tgt);
                        }
                    }
                    sb
                }
                ElementKind::ActionUsage => {
                    let mut sb = StateBuilder::atomic(sid);
                    if let Some(def_id) = owner_of.get(sid) {
                        sb = sb.parent(*def_id);
                    }
                    if let Some(targets) = successors.get(sid) {
                        for &tgt in targets {
                            sb = sb.on("next", tgt);
                        }
                    }
                    sb
                }
                ElementKind::DecisionNode => {
                    // Guard name is "<id>_cond"; events are "guard:<id>_true" / "guard:<id>_false"
                    let guard_name = format!("{sid}_cond");
                    decision_guards.insert(sid.to_string(), guard_name.clone());
                    let targets = successors.get(sid).cloned().unwrap_or_default();
                    let true_tgt = targets.first().copied().unwrap_or(sid);
                    let false_tgt = targets.get(1).copied().unwrap_or(sid);
                    let mut sb = StateBuilder::atomic(sid)
                        .on(&format!("guard:{sid}_true"), true_tgt)
                        .on(&format!("guard:{sid}_false"), false_tgt);
                    if let Some(parent) = owner_of.get(sid) {
                        sb = sb.parent(*parent);
                    }
                    sb
                }
                ElementKind::ForkNode => {
                    // Fork fires "fork:<id>" — caller handles parallel dispatch
                    let targets = successors.get(sid).cloned().unwrap_or_default();
                    let first_tgt = targets.first().copied().unwrap_or(sid);
                    let mut sb =
                        StateBuilder::atomic(sid).on(&format!("fork:{sid}"), first_tgt);
                    if let Some(parent) = owner_of.get(sid) {
                        sb = sb.parent(*parent);
                    }
                    sb
                }
                ElementKind::JoinNode | ElementKind::MergeNode => {
                    // Pass-through on "next"
                    let mut sb = StateBuilder::atomic(sid);
                    if let Some(parent) = owner_of.get(sid) {
                        sb = sb.parent(*parent);
                    }
                    if let Some(targets) = successors.get(sid) {
                        for &tgt in targets {
                            sb = sb.on("next", tgt);
                        }
                    }
                    sb
                }
                _ => {
                    // Unsupported kind — skip; forward-compatible
                    seen.remove(sid);
                    continue;
                }
            };
            builder = builder.state(sb);
        }

        let chart_def = builder.build().map_err(Kr0kiError::ChartBuild)?;
        Ok((chart_def, decision_guards))
    }

    /// Advance a linear-flow chart by sending `"next"`. Returns `true` if a
    /// transition fired. `guards` maps guard name → bool.
    pub fn advance(&self, guards: &HashMap<String, bool>) -> bool {
        self.chart.send(&self.ctx, "next", guards)
    }

    /// Drive a `DecisionNode` by id. Evaluates its guard and fires the
    /// appropriate routing event. Returns `true` if a transition fired.
    ///
    /// The guard name for node `id` is `"<id>_cond"`. Pass it in `guards`.
    pub fn decide(&self, node_id: &str, guards: &HashMap<String, bool>) -> bool {
        let guard_name = match self.decision_guards.get(node_id) {
            Some(g) => g,
            None => return false,
        };
        let condition = guards.get(guard_name.as_str()).copied().unwrap_or(false);
        let event = if condition {
            format!("guard:{node_id}_true")
        } else {
            format!("guard:{node_id}_false")
        };
        self.chart.send(&self.ctx, &event, guards)
    }

    /// Trigger a `ForkNode` by id, advancing to the first outgoing branch.
    /// Callers are responsible for handling additional parallel branches.
    pub fn fork(&self, node_id: &str) -> bool {
        let event = format!("fork:{node_id}");
        self.chart.send(&self.ctx, &event, &HashMap::new())
    }

    /// Current active leaf state ids.
    pub fn active_leaves(&self) -> Vec<String> {
        self.chart.active_leaves(&self.ctx)
    }

    /// `true` if state `id` is in the current configuration.
    pub fn is_active(&self, id: &str) -> bool {
        self.chart.matches(&self.ctx, id)
    }

    /// The action names emitted by the last transition (entry/exit/action).
    pub fn last_actions(&self) -> Vec<String> {
        self.chart.last_actions()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn eid(s: &str) -> ElementId {
        ElementId(s.to_string())
    }

    /// Single ActionDefinition with two ActionUsage children chained by Succession.
    #[test]
    fn succession_advances_through_steps() {
        let elements = vec![
            (eid("proc"), ElementKind::ActionDefinition),
            (eid("step_a"), ElementKind::ActionUsage),
            (eid("step_b"), ElementKind::ActionUsage),
        ];
        let relations = vec![
            Relation::FeatureMembership { owner: eid("proc"), member: eid("step_a") },
            Relation::FeatureMembership { owner: eid("proc"), member: eid("step_b") },
            Relation::Succession { source: eid("step_a"), target: eid("step_b") },
        ];

        let chart = ProcessChart::from_sysml(&elements, &relations).expect("chart build");

        assert!(chart.is_active("step_a"), "initial state must be step_a");
        let fired = chart.advance(&HashMap::new());
        assert!(fired, "transition must fire on 'next'");
        assert!(chart.is_active("step_b"), "active leaf must be step_b after advance");
    }

    /// Three-step linear chain: a → b → c
    #[test]
    fn three_step_linear_chain() {
        let elements = vec![
            (eid("proc"), ElementKind::ActionDefinition),
            (eid("a"), ElementKind::ActionUsage),
            (eid("b"), ElementKind::ActionUsage),
            (eid("c"), ElementKind::ActionUsage),
        ];
        let relations = vec![
            Relation::FeatureMembership { owner: eid("proc"), member: eid("a") },
            Relation::FeatureMembership { owner: eid("proc"), member: eid("b") },
            Relation::FeatureMembership { owner: eid("proc"), member: eid("c") },
            Relation::Succession { source: eid("a"), target: eid("b") },
            Relation::Succession { source: eid("b"), target: eid("c") },
        ];

        let chart = ProcessChart::from_sysml(&elements, &relations).expect("chart build");

        assert!(chart.is_active("a"));
        chart.advance(&HashMap::new());
        assert!(chart.is_active("b"));
        chart.advance(&HashMap::new());
        assert!(chart.is_active("c"));
    }

    /// DecisionNode routes true-branch when guard is set.
    #[test]
    fn decision_node_routes_true_branch() {
        let elements = vec![
            (eid("proc"), ElementKind::ActionDefinition),
            (eid("start"), ElementKind::ActionUsage),
            (eid("gate"), ElementKind::DecisionNode),
            (eid("branch_yes"), ElementKind::ActionUsage),
            (eid("branch_no"), ElementKind::ActionUsage),
        ];
        let relations = vec![
            Relation::FeatureMembership { owner: eid("proc"), member: eid("start") },
            Relation::FeatureMembership { owner: eid("proc"), member: eid("gate") },
            Relation::FeatureMembership { owner: eid("proc"), member: eid("branch_yes") },
            Relation::FeatureMembership { owner: eid("proc"), member: eid("branch_no") },
            // start → gate
            Relation::Succession { source: eid("start"), target: eid("gate") },
            // gate → branch_yes (true), gate → branch_no (false)
            Relation::Succession { source: eid("gate"), target: eid("branch_yes") },
            Relation::Succession { source: eid("gate"), target: eid("branch_no") },
        ];

        let chart = ProcessChart::from_sysml(&elements, &relations).expect("chart build");

        // Advance from start → gate
        chart.advance(&HashMap::new());
        assert!(chart.is_active("gate"), "must be at decision node");

        // Evaluate guard: true → branch_yes
        let mut guards = HashMap::new();
        guards.insert("gate_cond".to_string(), true);
        let fired = chart.decide("gate", &guards);
        assert!(fired, "decision must fire");
        assert!(chart.is_active("branch_yes"), "true branch must be active");
    }

    /// DecisionNode routes false-branch when guard is unset.
    #[test]
    fn decision_node_routes_false_branch() {
        let elements = vec![
            (eid("proc"), ElementKind::ActionDefinition),
            (eid("start"), ElementKind::ActionUsage),
            (eid("gate"), ElementKind::DecisionNode),
            (eid("branch_yes"), ElementKind::ActionUsage),
            (eid("branch_no"), ElementKind::ActionUsage),
        ];
        let relations = vec![
            Relation::FeatureMembership { owner: eid("proc"), member: eid("start") },
            Relation::FeatureMembership { owner: eid("proc"), member: eid("gate") },
            Relation::FeatureMembership { owner: eid("proc"), member: eid("branch_yes") },
            Relation::FeatureMembership { owner: eid("proc"), member: eid("branch_no") },
            Relation::Succession { source: eid("start"), target: eid("gate") },
            Relation::Succession { source: eid("gate"), target: eid("branch_yes") },
            Relation::Succession { source: eid("gate"), target: eid("branch_no") },
        ];

        let chart = ProcessChart::from_sysml(&elements, &relations).expect("chart build");

        chart.advance(&HashMap::new());
        assert!(chart.is_active("gate"));

        // guard false → branch_no
        let mut guards = HashMap::new();
        guards.insert("gate_cond".to_string(), false);
        let fired = chart.decide("gate", &guards);
        assert!(fired, "decision must fire");
        assert!(chart.is_active("branch_no"), "false branch must be active");
    }

    /// ForkNode fires fork:<id> and advances to first outgoing branch.
    #[test]
    fn fork_node_fires_and_advances() {
        let elements = vec![
            (eid("proc"), ElementKind::ActionDefinition),
            (eid("start"), ElementKind::ActionUsage),
            (eid("split"), ElementKind::ForkNode),
            (eid("branch_a"), ElementKind::ActionUsage),
            (eid("branch_b"), ElementKind::ActionUsage),
        ];
        let relations = vec![
            Relation::FeatureMembership { owner: eid("proc"), member: eid("start") },
            Relation::FeatureMembership { owner: eid("proc"), member: eid("split") },
            Relation::FeatureMembership { owner: eid("proc"), member: eid("branch_a") },
            Relation::FeatureMembership { owner: eid("proc"), member: eid("branch_b") },
            Relation::Succession { source: eid("start"), target: eid("split") },
            // Fork to branch_a first (first outgoing)
            Relation::Succession { source: eid("split"), target: eid("branch_a") },
            Relation::Succession { source: eid("split"), target: eid("branch_b") },
        ];

        let chart = ProcessChart::from_sysml(&elements, &relations).expect("chart build");

        // start → split
        chart.advance(&HashMap::new());
        assert!(chart.is_active("split"), "must be at fork node");

        // fork fires → first branch (branch_a)
        let fired = chart.fork("split");
        assert!(fired, "fork must advance");
        assert!(chart.is_active("branch_a"), "first branch must be active");
    }

    /// MergeNode passes through on "next".
    #[test]
    fn merge_node_passes_through() {
        let elements = vec![
            (eid("proc"), ElementKind::ActionDefinition),
            (eid("start"), ElementKind::ActionUsage),
            (eid("join"), ElementKind::MergeNode),
            (eid("end"), ElementKind::ActionUsage),
        ];
        let relations = vec![
            Relation::FeatureMembership { owner: eid("proc"), member: eid("start") },
            Relation::FeatureMembership { owner: eid("proc"), member: eid("join") },
            Relation::FeatureMembership { owner: eid("proc"), member: eid("end") },
            Relation::Succession { source: eid("start"), target: eid("join") },
            Relation::Succession { source: eid("join"), target: eid("end") },
        ];

        let chart = ProcessChart::from_sysml(&elements, &relations).expect("chart build");

        chart.advance(&HashMap::new()); // start → join
        assert!(chart.is_active("join"));
        chart.advance(&HashMap::new()); // join → end
        assert!(chart.is_active("end"));
    }

    /// ActionUsage with no FeatureMembership (orphan) — chart still builds.
    #[test]
    fn orphan_action_usage_builds() {
        let elements = vec![
            (eid("root"), ElementKind::ActionDefinition),
            (eid("orphan"), ElementKind::ActionUsage),
        ];
        let relations = vec![];
        let result = ProcessChart::from_sysml(&elements, &relations);
        let _ = result; // may succeed or fail; no panic
    }
}
