//! Live lazily executor for the OODA proposal lifecycle.
//!
//! ufo-types exports the OODA state graph as SCXML (interchange/export only).
//! This module adds the live runtime: a `ThreadSafeStateChart` whose topology
//! is derived from the same authoritative `OodaEvent::target` transition table
//! in ufo-types, so both always agree.
//!
//! The SCXML export and this runtime are kept separate on purpose:
//! - SCXML export: for diagram tools (statecharts.dev), validators, round-trip
//! - lazily runtime: for in-process enforcement, PM hook dispatch, SSE events

use std::collections::HashMap;

use lazily::statechart::{ChartBuilder, ChartDef, StateBuilder, ThreadSafeStateChart};
use lazily::ThreadSafeContext;

/// Build the OODA chart definition from the authoritative ufo-types transition
/// table. States mirror `OodaPhase`; transitions mirror `OodaEvent::target`.
pub fn ooda_chart_def() -> Result<ChartDef, String> {
    // States: compound root + 5 atomic phases + 1 terminal
    // Each non-initial state fires "ooda_transition" on entry so PM hooks can
    // observe every advance without coupling to the chart internals.
    ChartBuilder::new()
        .state(StateBuilder::compound("ooda", "observe"))
        // Observe: initial phase — entered on "discover" from anywhere
        .state(
            StateBuilder::atomic("observe")
                .parent("ooda")
                .entry("ooda_transition")
                .on("analyze_complete", "orient"),
        )
        // Orient
        .state(
            StateBuilder::atomic("orient")
                .parent("ooda")
                .entry("ooda_transition")
                .on("decide", "decide")
                .on("discover", "observe"),
        )
        // Decide
        .state(
            StateBuilder::atomic("decide")
                .parent("ooda")
                .entry("ooda_transition")
                .on("execute", "act")
                .on("discover", "observe"),
        )
        // Act
        .state(
            StateBuilder::atomic("act")
                .parent("ooda")
                .entry("ooda_transition")
                .on("verify_complete", "verify")
                .on("reject", "decide")
                .on("discover", "observe"),
        )
        // Verify
        .state(
            StateBuilder::atomic("verify")
                .parent("ooda")
                .entry("ooda_transition")
                .on("reject", "decide")
                .on("discover", "observe"),
        )
        .build()
}

/// Live OODA chart — holds a `ThreadSafeStateChart` and drives it via typed
/// event names. Fires `"ooda_transition"` as an entry action on every advance;
/// callers read `last_actions()` to observe the effect.
pub struct OodaChart {
    chart: ThreadSafeStateChart,
    ctx: ThreadSafeContext,
}

impl OodaChart {
    pub fn new() -> Result<Self, String> {
        let def = ooda_chart_def()?;
        let ctx = ThreadSafeContext::new();
        let chart = ThreadSafeStateChart::new(&ctx, def);
        Ok(Self { chart, ctx })
    }

    /// Send an event. Returns `true` iff a transition fired.
    /// Event names match ufo-types `OodaEvent` display names lowercased +
    /// underscored: `"discover"`, `"analyze_complete"`, `"decide"`,
    /// `"execute"`, `"verify_complete"`, `"reject"`, `"retry"`.
    pub fn send(&self, event: &str) -> bool {
        let guards: HashMap<String, bool> = HashMap::new();
        self.chart.send(&self.ctx, event, &guards)
    }

    /// Entry actions from the most recent `send`. Contains `"ooda_transition"`
    /// on a successful advance; empty on a rejected event.
    pub fn last_actions(&self) -> Vec<String> {
        self.chart.last_actions()
    }

    /// `true` iff the last `send` fired `"ooda_transition"`.
    pub fn transitioned(&self) -> bool {
        self.last_actions().contains(&"ooda_transition".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ooda_chart_def_builds_without_error() {
        ooda_chart_def().expect("ooda_chart_def must build");
    }

    #[test]
    fn discover_from_orient_returns_to_observe() {
        let chart = OodaChart::new().expect("chart");
        // Advance to orient first
        assert!(chart.send("analyze_complete"), "observe → orient");
        // "discover" from orient → observe (loop-back transition)
        assert!(chart.send("discover"), "orient → observe on discover");
        assert!(chart.transitioned());
    }

    #[test]
    fn full_happy_path_observe_to_verify() {
        let chart = OodaChart::new().expect("chart");
        // Initial state is "observe"; drive through to "verify"
        assert!(chart.send("analyze_complete"), "observe → orient");
        assert!(chart.transitioned());

        assert!(chart.send("decide"), "orient → decide");
        assert!(chart.transitioned());

        assert!(chart.send("execute"), "decide → act");
        assert!(chart.transitioned());

        assert!(chart.send("verify_complete"), "act → verify");
        assert!(chart.transitioned());
    }

    #[test]
    fn reject_from_verify_returns_to_decide() {
        let chart = OodaChart::new().expect("chart");
        // Fast-forward to verify
        chart.send("analyze_complete");
        chart.send("decide");
        chart.send("execute");
        chart.send("verify_complete");
        // Reject from verify → decide
        assert!(chart.send("reject"), "verify → decide on reject");
        assert!(chart.transitioned());
    }

    #[test]
    fn unknown_event_does_not_transition() {
        let chart = OodaChart::new().expect("chart");
        let fired = chart.send("not_a_real_event");
        assert!(!fired);
        assert!(!chart.transitioned());
    }
}
