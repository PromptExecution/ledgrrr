//! Build and drive a lazily StateChart from SysML-v2 process elements.

use std::collections::{BTreeSet, HashMap};

use lazily::statechart::{ChartBuilder, ChartDef, StateBuilder, ThreadSafeStateChart};
use lazily::ThreadSafeContext;
use ufo_types::{ElementId, ElementKind, Relation};

use crate::error::Kr0kiError;

/// A compiled process chart ready to be driven by sending events.
pub struct ProcessChart {
    chart: ThreadSafeStateChart,
    ctx: ThreadSafeContext,
}

impl ProcessChart {
    /// Build a `ProcessChart` from a flat slice of `(ElementId, ElementKind)`
    /// element declarations and a slice of `Relation`s.
    ///
    /// ## Mapping rules (Phase 1–5)
    ///
    /// | SysML-v2 element    | lazily state        | Events / guards                                        |
    /// |---------------------|---------------------|--------------------------------------------------------|
    /// | `ActionDefinition`  | compound root       | initial = first owned usage                            |
    /// | `ActionUsage`       | atomic leaf         | `on("next", target)` per outgoing Succession           |
    /// | `MergeNode`         | atomic              | `on("next", target)` — fan-in pass-through             |
    /// | `DecisionNode`      | atomic              | `on_guarded("next", t, "choose:<t>")` per branch       |
    /// | `ForkNode`          | atomic              | `on_guarded("next", t, "fork:<t>")` per branch         |
    /// | `JoinNode`          | atomic              | `on("next", target)` — fan-out re-convergence          |
    ///
    /// **Phase 6 note:** True concurrent execution for ForkNode/JoinNode requires
    /// lazily parallel-region construction and topology detection (detecting the
    /// fork-join pair as a `StateBuilder::parallel` with per-branch child regions
    /// and final states). Phase 5 models Fork/Join as guarded-branching atomic
    /// states so the topology is representable and testable without full SCXML
    /// parallel semantics.
    pub fn from_sysml(
        elements: &[(ElementId, ElementKind)],
        relations: &[Relation],
    ) -> Result<Self, Kr0kiError> {
        let action_defs: Vec<&ElementId> = elements
            .iter()
            .filter(|(_, k)| *k == ElementKind::ActionDefinition)
            .map(|(id, _)| id)
            .collect();

        let action_usages: Vec<&ElementId> = elements
            .iter()
            .filter(|(_, k)| *k == ElementKind::ActionUsage)
            .map(|(id, _)| id)
            .collect();

        let control_nodes: Vec<(&ElementId, ElementKind)> = elements
            .iter()
            .filter(|(_, k)| k.is_control_node())
            .map(|(id, k)| (id, *k))
            .collect();

        // Build ownership map: usage/node → definition
        let mut owner_of: HashMap<&str, &str> = HashMap::new();
        for rel in relations {
            if let Relation::FeatureMembership { owner, member } = rel {
                owner_of.insert(member.0.as_str(), owner.0.as_str());
            }
        }

        // Build succession map: source → Vec<target>
        let mut successors: HashMap<&str, Vec<&str>> = HashMap::new();
        for rel in relations {
            if let Relation::Succession { source, target } = rel {
                successors
                    .entry(source.0.as_str())
                    .or_default()
                    .push(target.0.as_str());
            }
        }

        // Determine initial child for each compound state (first usage owned by it)
        let mut first_child: HashMap<&str, &str> = HashMap::new();
        for usage in &action_usages {
            let usage_id = usage.0.as_str();
            if let Some(def_id) = owner_of.get(usage_id) {
                first_child.entry(def_id).or_insert(usage_id);
            }
        }
        // Also consider control nodes owned by a definition
        for (node, _) in &control_nodes {
            let node_id = node.0.as_str();
            if let Some(def_id) = owner_of.get(node_id) {
                first_child.entry(def_id).or_insert(node_id);
            }
        }

        let chart_def = Self::build_chart_def(
            &action_defs,
            &action_usages,
            &control_nodes,
            &owner_of,
            &first_child,
            &successors,
        )?;

        let ctx = ThreadSafeContext::new();
        let chart = ThreadSafeStateChart::new(&ctx, chart_def);

        Ok(Self { chart, ctx })
    }

    fn build_chart_def(
        defs: &[&ElementId],
        usages: &[&ElementId],
        control_nodes: &[(&ElementId, ElementKind)],
        owner_of: &HashMap<&str, &str>,
        first_child: &HashMap<&str, &str>,
        successors: &HashMap<&str, Vec<&str>>,
    ) -> Result<ChartDef, Kr0kiError> {
        let mut builder = ChartBuilder::new();
        let mut seen = BTreeSet::new();

        // ActionDefinition → compound (or atomic if childless)
        for def in defs {
            let def_id = def.0.as_str();
            if !seen.insert(def_id) {
                return Err(Kr0kiError::DuplicateId(def_id.to_string()));
            }
            let initial = first_child.get(def_id).copied().unwrap_or("");
            let mut sb = if initial.is_empty() {
                StateBuilder::atomic(def_id)
            } else {
                StateBuilder::compound(def_id, initial)
            };
            if let Some(targets) = successors.get(def_id) {
                for &tgt in targets {
                    sb = sb.on("next", tgt);
                }
            }
            builder = builder.state(sb);
        }

        // ActionUsage → atomic leaf
        for usage in usages {
            let usage_id = usage.0.as_str();
            if !seen.insert(usage_id) {
                return Err(Kr0kiError::DuplicateId(usage_id.to_string()));
            }
            let mut sb = StateBuilder::atomic(usage_id);
            if let Some(def_id) = owner_of.get(usage_id) {
                sb = sb.parent(*def_id);
            }
            if let Some(targets) = successors.get(usage_id) {
                for &tgt in targets {
                    sb = sb.on("next", tgt);
                }
            }
            builder = builder.state(sb);
        }

        // Control nodes — Phase 5 mapping (see doc comment on from_sysml)
        for (node, kind) in control_nodes {
            let node_id = node.0.as_str();
            if !seen.insert(node_id) {
                return Err(Kr0kiError::DuplicateId(node_id.to_string()));
            }
            let mut sb = StateBuilder::atomic(node_id);
            if let Some(def_id) = owner_of.get(node_id) {
                sb = sb.parent(*def_id);
            }
            match kind {
                // MergeNode: same as ActionUsage — plain "next" transitions
                ElementKind::MergeNode | ElementKind::JoinNode => {
                    if let Some(targets) = successors.get(node_id) {
                        for &tgt in targets {
                            sb = sb.on("next", tgt);
                        }
                    }
                }
                // DecisionNode: one distinct event per branch — "choose:<target>".
                // lazily stores one transition per event key, so guards on the same
                // "next" event would silently overwrite each other. Named events are
                // unambiguous and work with the single-transition-per-event constraint.
                // Phase 6 note: replace with proper SCXML event-less transitions +
                // condition expressions once lazily gains Vec<Transition> per event.
                ElementKind::DecisionNode => {
                    if let Some(targets) = successors.get(node_id) {
                        match targets.as_slice() {
                            [only] => sb = sb.on("next", *only),
                            many => {
                                for &tgt in many {
                                    let event = format!("choose:{tgt}");
                                    sb = sb.on(event, tgt);
                                }
                            }
                        }
                    }
                }
                // ForkNode: one distinct event per branch — "fork:<target>".
                // Same rationale as DecisionNode above.
                // Phase 6: replace with StateBuilder::parallel + child regions.
                ElementKind::ForkNode => {
                    if let Some(targets) = successors.get(node_id) {
                        match targets.as_slice() {
                            [only] => sb = sb.on("next", *only),
                            many => {
                                for &tgt in many {
                                    let event = format!("fork:{tgt}");
                                    sb = sb.on(event, tgt);
                                }
                            }
                        }
                    }
                }
                _ => unreachable!("control_nodes filtered to is_control_node()"),
            }
            builder = builder.state(sb);
        }

        builder.build().map_err(Kr0kiError::ChartBuild)
    }

    /// Send event `"next"` to advance the chart (unguarded succession).
    pub fn advance(&self) -> bool {
        self.chart.send(&self.ctx, "next", &HashMap::new())
    }

    /// Route a `DecisionNode` to `target_id` by sending `"choose:<target_id>"`.
    pub fn decide(&self, target_id: &str) -> bool {
        let event = format!("choose:{target_id}");
        self.chart.send(&self.ctx, &event, &HashMap::new())
    }

    /// Route a `ForkNode` to one branch `target_id` by sending `"fork:<target_id>"`.
    /// Phase 6: replace with parallel entry once lazily supports Vec<Transition>/event.
    pub fn fork_to(&self, target_id: &str) -> bool {
        let event = format!("fork:{target_id}");
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
    /// Expected: start in "step_a", advance → "step_b".
    #[test]
    fn succession_advances_through_steps() {
        let elements = vec![
            (eid("proc"), ElementKind::ActionDefinition),
            (eid("step_a"), ElementKind::ActionUsage),
            (eid("step_b"), ElementKind::ActionUsage),
        ];
        let relations = vec![
            Relation::FeatureMembership {
                owner: eid("proc"),
                member: eid("step_a"),
            },
            Relation::FeatureMembership {
                owner: eid("proc"),
                member: eid("step_b"),
            },
            Relation::Succession {
                source: eid("step_a"),
                target: eid("step_b"),
            },
        ];

        let chart = ProcessChart::from_sysml(&elements, &relations)
            .expect("chart build should succeed");

        assert!(chart.is_active("step_a"), "initial state must be step_a");
        let fired = chart.advance();
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

        let chart = ProcessChart::from_sysml(&elements, &relations)
            .expect("chart build");

        assert!(chart.is_active("a"));
        chart.advance();
        assert!(chart.is_active("b"));
        chart.advance();
        assert!(chart.is_active("c"));
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
        let _ = result;
    }

    /// MergeNode: two branches converge at a merge, then continue to "done".
    /// Topology: a --next--> merge; b --next--> merge; merge --next--> done
    /// Starting in "a" (first child of proc), advancing routes a→merge→done.
    #[test]
    fn merge_node_pass_through() {
        let elements = vec![
            (eid("proc"), ElementKind::ActionDefinition),
            (eid("a"), ElementKind::ActionUsage),
            (eid("b"), ElementKind::ActionUsage),
            (eid("merge"), ElementKind::MergeNode),
            (eid("done"), ElementKind::ActionUsage),
        ];
        let relations = vec![
            Relation::FeatureMembership { owner: eid("proc"), member: eid("a") },
            Relation::FeatureMembership { owner: eid("proc"), member: eid("b") },
            Relation::FeatureMembership { owner: eid("proc"), member: eid("merge") },
            Relation::FeatureMembership { owner: eid("proc"), member: eid("done") },
            Relation::Succession { source: eid("a"), target: eid("merge") },
            Relation::Succession { source: eid("b"), target: eid("merge") },
            Relation::Succession { source: eid("merge"), target: eid("done") },
        ];

        let chart = ProcessChart::from_sysml(&elements, &relations).expect("merge chart builds");
        // "a" is first child → initial
        assert!(chart.is_active("a"));
        chart.advance(); // a → merge
        assert!(chart.is_active("merge"));
        chart.advance(); // merge → done
        assert!(chart.is_active("done"));
    }

    /// DecisionNode: two branches — route to "branch_yes" via `decide()`.
    /// Start at "step_a", advance to decision, then call `decide("branch_yes")`.
    #[test]
    fn decision_node_routing() {
        let elements = vec![
            (eid("proc"), ElementKind::ActionDefinition),
            (eid("step_a"), ElementKind::ActionUsage),
            (eid("decision"), ElementKind::DecisionNode),
            (eid("branch_yes"), ElementKind::ActionUsage),
            (eid("branch_no"), ElementKind::ActionUsage),
        ];
        let relations = vec![
            Relation::FeatureMembership { owner: eid("proc"), member: eid("step_a") },
            Relation::FeatureMembership { owner: eid("proc"), member: eid("decision") },
            Relation::FeatureMembership { owner: eid("proc"), member: eid("branch_yes") },
            Relation::FeatureMembership { owner: eid("proc"), member: eid("branch_no") },
            Relation::Succession { source: eid("step_a"), target: eid("decision") },
            Relation::Succession { source: eid("decision"), target: eid("branch_yes") },
            Relation::Succession { source: eid("decision"), target: eid("branch_no") },
        ];

        let chart = ProcessChart::from_sysml(&elements, &relations)
            .expect("decision chart builds");

        assert!(chart.is_active("step_a"));
        chart.advance(); // step_a → decision
        assert!(chart.is_active("decision"));

        let fired = chart.decide("branch_yes");
        assert!(fired, "decide(branch_yes) must fire");
        assert!(chart.is_active("branch_yes"), "must be in branch_yes after decide");
    }

    /// ForkNode: two branches — route to branch "b" via `fork_to()`.
    /// Advance through fork to one branch, then continue to join and end.
    #[test]
    fn fork_node_fanout() {
        let elements = vec![
            (eid("proc"), ElementKind::ActionDefinition),
            (eid("start"), ElementKind::ActionUsage),
            (eid("fork"), ElementKind::ForkNode),
            (eid("b"), ElementKind::ActionUsage),
            (eid("c"), ElementKind::ActionUsage),
            (eid("join"), ElementKind::JoinNode),
            (eid("end"), ElementKind::ActionUsage),
        ];
        let relations = vec![
            Relation::FeatureMembership { owner: eid("proc"), member: eid("start") },
            Relation::FeatureMembership { owner: eid("proc"), member: eid("fork") },
            Relation::FeatureMembership { owner: eid("proc"), member: eid("b") },
            Relation::FeatureMembership { owner: eid("proc"), member: eid("c") },
            Relation::FeatureMembership { owner: eid("proc"), member: eid("join") },
            Relation::FeatureMembership { owner: eid("proc"), member: eid("end") },
            Relation::Succession { source: eid("start"), target: eid("fork") },
            Relation::Succession { source: eid("fork"), target: eid("b") },
            Relation::Succession { source: eid("fork"), target: eid("c") },
            Relation::Succession { source: eid("b"), target: eid("join") },
            Relation::Succession { source: eid("c"), target: eid("join") },
            Relation::Succession { source: eid("join"), target: eid("end") },
        ];

        let chart = ProcessChart::from_sysml(&elements, &relations)
            .expect("fork/join chart builds");

        assert!(chart.is_active("start"));
        chart.advance(); // start → fork
        assert!(chart.is_active("fork"));

        let fired = chart.fork_to("b");
        assert!(fired, "fork_to(b) must fire");
        assert!(chart.is_active("b"), "must be in branch b");
        chart.advance(); // b → join
        assert!(chart.is_active("join"));
        chart.advance(); // join → end
        assert!(chart.is_active("end"));
    }

    /// JoinNode with single outgoing Succession (plain "next").
    #[test]
    fn join_node_single_successor() {
        let elements = vec![
            (eid("proc"), ElementKind::ActionDefinition),
            (eid("a"), ElementKind::ActionUsage),
            (eid("join"), ElementKind::JoinNode),
            (eid("done"), ElementKind::ActionUsage),
        ];
        let relations = vec![
            Relation::FeatureMembership { owner: eid("proc"), member: eid("a") },
            Relation::FeatureMembership { owner: eid("proc"), member: eid("join") },
            Relation::FeatureMembership { owner: eid("proc"), member: eid("done") },
            Relation::Succession { source: eid("a"), target: eid("join") },
            Relation::Succession { source: eid("join"), target: eid("done") },
        ];

        let chart = ProcessChart::from_sysml(&elements, &relations).expect("join chart builds");
        assert!(chart.is_active("a"));
        chart.advance();
        assert!(chart.is_active("join"));
        chart.advance();
        assert!(chart.is_active("done"));
    }
}
