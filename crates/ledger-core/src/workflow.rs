//! TOML workflow DSL compiler.
//! Single source of truth for pipeline workflow definitions.
//! Generates Rhai FSM, Rust enum, and Mermaid diagram.

use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// Workflow definition from TOML.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WorkflowToml {
    pub name: String,
    pub version: String,
    pub state: Vec<StateDecl>,
    #[serde(rename = "transition")]
    pub transitions: Vec<TransitionDecl>,
}

/// State declaration in workflow.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct StateDecl {
    pub id: String,
    pub initial: Option<bool>,
    pub terminal: Option<bool>,
    pub is_error: Option<bool>,
}

/// Transition declaration.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TransitionDecl {
    pub from: String,
    pub event: String,
    pub guard: Option<String>,
    pub to: String,
    #[serde(rename = "else")]
    pub else_to: Option<String>,
}

impl WorkflowToml {
    /// Validate the workflow structure.
    pub fn validate(&self) -> Result<(), Vec<String>> {
        let mut errors = vec![];
        let state_ids: HashSet<_> = self.state.iter().map(|s| &s.id).collect();

        // Check all transitions reference declared states
        for t in &self.transitions {
            if !state_ids.contains(&t.from) {
                errors.push(format!("unknown from-state: {}", t.from));
            }
            if !state_ids.contains(&t.to) {
                errors.push(format!("unknown to-state: {}", t.to));
            }
            if let Some(e) = &t.else_to {
                if !state_ids.contains(e) {
                    errors.push(format!("unknown else-state: {e}"));
                }
            }
            // Guard without else
            if t.guard.is_some() && t.else_to.is_none() {
                errors.push(format!(
                    "transition {}→{} has guard but no else",
                    t.from, t.to
                ));
            }
        }

        // Check exactly one initial
        let initials: Vec<_> = self
            .state
            .iter()
            .filter(|s| s.initial == Some(true))
            .collect();
        if initials.len() != 1 {
            errors.push(format!(
                "exactly one initial state required, found {}",
                initials.len()
            ));
        }

        // Check non-terminal states have outgoing transitions
        for s in self.state.iter().filter(|s| s.terminal != Some(true)) {
            if !self.transitions.iter().any(|t| t.from == s.id) {
                errors.push(format!(
                    "non-terminal state {} has no outgoing transitions",
                    s.id
                ));
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    /// Compile to Mermaid stateDiagram-v2.
    pub fn to_mermaid(&self) -> String {
        let mut out = format!(
            "%%{{ init: {{ 'theme': 'neutral' }} }}%%\nstateDiagram-v2\n\
             %% Generated from workflows/{}.toml\n",
            self.name
        );

        // Initial state
        for s in self.state.iter().filter(|s| s.initial == Some(true)) {
            out += &format!("    [*] --> {}\n", s.id);
        }

        // Transitions
        for t in &self.transitions {
            let label = match &t.guard {
                None => t.event.clone(),
                Some(g) => format!("{} [{}]", t.event, g),
            };
            out += &format!("    {} --> {} : {}\n", t.from, t.to, label);
            if let Some(e) = &t.else_to {
                out += &format!("    {} --> {} : {} [else]\n", t.from, e, t.event);
            }
        }

        // Terminal states
        for s in self.state.iter().filter(|s| s.terminal == Some(true)) {
            out += &format!("    {} --> [*]\n", s.id);
        }

        out
    }

    /// Compile to Rhai next_state function.
    pub fn to_rhai(&self) -> String {
        let mut arms = Vec::new();
        for t in &self.transitions {
            let from = &t.from;
            let to = &t.to;
            let event = &t.event;

            let arm = match &t.guard {
                None => format!("        [\"{}\", \"{}\"] => \"{}\"", from, event, to),
                Some(g) => {
                    let else_to = t.else_to.as_deref().unwrap_or(to);
                    format!(
                        "        [\"{}\", \"{}\"] => if {} then \"{}\" else \"{}\"",
                        from, event, g, to, else_to
                    )
                }
            };
            arms.push(arm);
        }

        let body = arms.join(",\n");
        let mut result = String::new();
        result.push_str("// GENERATED from workflows/");
        result.push_str(&self.name);
        result.push_str("\nfn next_state(state, event, ctx) {\n");
        result.push_str("    switch [state, event.kind] {\n");
        result.push_str(&body);
        result.push_str(",\n        _ => { error: \"no transition\", terminal: true }\n");
        result.push_str("    }\n");
        result.push_str("}\n");
        result
    }

    /// Compile to a W3C SCXML statechart (_b00t_#1177 P7), alongside the
    /// existing to_mermaid/to_rhai/to_rust_enum codegen targets.
    ///
    /// A guarded transition with an `else` branch becomes two SCXML
    /// transitions on the same event, in document order: the guarded one
    /// first, then a bare fallback to `else_to` — SCXML transition
    /// selection takes the first transition whose guard matches (or has
    /// no guard), so this preserves the TOML DSL's if/else semantics.
    /// A state is exported as `Final` only when it's both `terminal` and
    /// has zero outgoing transitions (SCXML's `Final` forbids outgoing
    /// transitions outright, stricter than this DSL's own `terminal`
    /// flag) — otherwise it's `Atomic`.
    #[cfg(feature = "statechart")]
    pub fn to_scxml(&self) -> scxml::model::Statechart {
        use scxml::model::{State, Transition};

        let initial = self
            .state
            .iter()
            .find(|s| s.initial == Some(true))
            .map(|s| s.id.as_str())
            .unwrap_or_default();

        let states: Vec<State> = self
            .state
            .iter()
            .map(|s| {
                let outgoing: Vec<&TransitionDecl> =
                    self.transitions.iter().filter(|t| t.from == s.id).collect();

                let mut state = if s.terminal == Some(true) && outgoing.is_empty() {
                    State::final_state(s.id.clone())
                } else {
                    State::atomic(s.id.clone())
                };

                state.transitions = outgoing
                    .into_iter()
                    .flat_map(|t| {
                        let mut transitions = vec![match &t.guard {
                            None => Transition::new(t.event.clone(), t.to.clone()),
                            Some(g) => {
                                Transition::new(t.event.clone(), t.to.clone()).with_guard(g.clone())
                            }
                        }];
                        if let Some(else_to) = &t.else_to {
                            transitions
                                .push(Transition::new(t.event.clone(), else_to.clone()));
                        }
                        transitions
                    })
                    .collect();

                state
            })
            .collect();

        scxml::model::Statechart::new(initial, states).with_name(self.name.clone())
    }

    /// Compile to Rust enum.
    pub fn to_rust_enum(&self) -> String {
        let variants: String = self
            .state
            .iter()
            .map(|s| format!("    {},\n", s.id))
            .collect();

        format!(
            "// GENERATED from workflows/{}.toml\n\
             #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, strum::EnumString, strum::Display)]\n\
             #[repr(u8)]\n\
             pub enum PipelineState {{\n\
             {variants}}}\n",
            self.name
        )
    }
}

/// Example: ledger_ingest workflow.
pub mod examples {
    use super::*;

    pub fn ledger_ingest() -> WorkflowToml {
        WorkflowToml {
            name: "ledger_ingest".to_string(),
            version: "1".to_string(),
            state: vec![
                StateDecl {
                    id: "Ingested".to_string(),
                    initial: Some(true),
                    terminal: None,
                    is_error: None,
                },
                StateDecl {
                    id: "Validating".to_string(),
                    initial: None,
                    terminal: None,
                    is_error: None,
                },
                StateDecl {
                    id: "Classifying".to_string(),
                    initial: None,
                    terminal: None,
                    is_error: None,
                },
                StateDecl {
                    id: "Reconciling".to_string(),
                    initial: None,
                    terminal: None,
                    is_error: None,
                },
                StateDecl {
                    id: "Committed".to_string(),
                    initial: None,
                    terminal: Some(true),
                    is_error: None,
                },
                StateDecl {
                    id: "NeedsReview".to_string(),
                    initial: None,
                    terminal: Some(true),
                    is_error: Some(true),
                },
            ],
            transitions: vec![
                TransitionDecl {
                    from: "Ingested".to_string(),
                    event: "SHAPE_DETECTED".to_string(),
                    guard: None,
                    to: "Validating".to_string(),
                    else_to: None,
                },
                TransitionDecl {
                    from: "Validating".to_string(),
                    event: "PASS".to_string(),
                    guard: None,
                    to: "Classifying".to_string(),
                    else_to: None,
                },
                TransitionDecl {
                    from: "Validating".to_string(),
                    event: "FAIL".to_string(),
                    guard: Some("ctx.repair_attempts < 2".to_string()),
                    to: "NeedsReview".to_string(),
                    else_to: Some("NeedsReview".to_string()),
                },
                TransitionDecl {
                    from: "Classifying".to_string(),
                    event: "HIGH_CONF".to_string(),
                    guard: None,
                    to: "Reconciling".to_string(),
                    else_to: None,
                },
                TransitionDecl {
                    from: "Classifying".to_string(),
                    event: "LOW_CONF".to_string(),
                    guard: None,
                    to: "NeedsReview".to_string(),
                    else_to: None,
                },
                TransitionDecl {
                    from: "Reconciling".to_string(),
                    event: "XERO_OK".to_string(),
                    guard: None,
                    to: "Committed".to_string(),
                    else_to: None,
                },
            ],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_workflow_validation_valid() {
        let wf = examples::ledger_ingest();
        assert!(wf.validate().is_ok());
    }

    #[test]
    fn test_to_mermaid() {
        let wf = examples::ledger_ingest();
        let mermaid = wf.to_mermaid();
        assert!(mermaid.contains("stateDiagram-v2"));
        assert!(mermaid.contains("Ingested"));
        assert!(mermaid.contains("Committed"));
    }

    #[test]
    fn test_to_rhai() {
        let wf = examples::ledger_ingest();
        let rhai = wf.to_rhai();
        assert!(rhai.contains("fn next_state"));
        assert!(rhai.contains("Ingested"));
    }

    #[test]
    fn test_to_rust_enum() {
        let wf = examples::ledger_ingest();
        let rust = wf.to_rust_enum();
        assert!(rust.contains("enum PipelineState"));
    }

    #[cfg(feature = "statechart")]
    #[test]
    fn test_to_scxml_round_trips_and_covers_every_real_transition() {
        use scxml::model::StateKind;

        let wf = examples::ledger_ingest();
        let chart = wf.to_scxml();
        scxml::validate(&chart).expect("ledger_ingest's exported statechart should be valid SCXML");

        let xml = scxml::export::xml::to_xml(&chart);
        let round_tripped = scxml::parse_xml(&xml).expect("round-tripped XML should re-parse");
        assert_eq!(chart, round_tripped);

        assert_eq!(round_tripped.initial.as_str(), "Ingested");

        // Committed and NeedsReview are both `terminal` with zero outgoing
        // transitions, so they export as real SCXML Final states.
        for terminal in ["Committed", "NeedsReview"] {
            let state = round_tripped
                .find_state(terminal)
                .unwrap_or_else(|| panic!("{terminal} should exist"));
            assert_eq!(state.kind, StateKind::Final, "{terminal} should be Final");
        }

        // Validating's guarded FAIL transition (with an `else` branch to the
        // same target) becomes two SCXML transitions on the same event.
        let validating = round_tripped.find_state("Validating").unwrap();
        let fail_transitions: Vec<_> = validating
            .transitions
            .iter()
            .filter(|t| t.event.as_deref() == Some("FAIL"))
            .collect();
        assert_eq!(fail_transitions.len(), 2, "guard + else should both be present");
        assert!(fail_transitions[0].guard.is_some(), "guarded arm comes first");
        assert!(fail_transitions[1].guard.is_none(), "bare fallback comes second");
        for t in &fail_transitions {
            assert_eq!(t.targets, vec!["NeedsReview".to_string()]);
        }

        // Every other real transition round-trips too.
        for (from, event, to) in [
            ("Ingested", "SHAPE_DETECTED", "Validating"),
            ("Validating", "PASS", "Classifying"),
            ("Classifying", "HIGH_CONF", "Reconciling"),
            ("Classifying", "LOW_CONF", "NeedsReview"),
            ("Reconciling", "XERO_OK", "Committed"),
        ] {
            let state = round_tripped.find_state(from).unwrap();
            assert!(
                state
                    .transitions
                    .iter()
                    .any(|t| t.event.as_deref() == Some(event) && t.targets == vec![to.to_string()]),
                "missing transition {from} --{event}--> {to}"
            );
        }
    }
}
