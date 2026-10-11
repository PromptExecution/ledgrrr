# Foundry Local Epic — Phase 5: Lifecycle + SCXML Statechart Export

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A real `FoundryLocalLifecycle` enum derived from Phase 4's `FoundryLocalStatus`, exported as a W3C SCXML statechart — the final phase of the Foundry Local epic, and a new milestone (P9) on `elasticdotventures/_b00t_#1177` (the SysML-v2/statechart spine epic this whole thread is an extension of).

**Architecture:** Two tasks in one new file, `ledgerr-desktop-agent/src/lifecycle.rs` — the enum + a genuine status-to-state derivation function (Task 1), then an SCXML export following `ufo-types::statechart::ooda_phases_to_statechart`'s exact established pattern (Task 2). Unlike `#1177`'s P5b-P8 (each of which migrated an *existing* hand-rolled state machine onto the shared `scxml` representation), there is no pre-existing Foundry Local FSM to migrate — this is new state-machine design, built directly on Phase 4's real detection logic rather than invented independently.

**Tech Stack:** Rust, `scxml` crate pinned to the exact version already used elsewhere in this ecosystem (`=0.2.2` — "small bus factor, 0.x semver" per the existing pin comment in `ledger-core/Cargo.toml` and `ufo-types/Cargo.toml`).

**Spec:** https://github.com/PromptExecution/ledgrrr/issues/220 (this repo's tracking issue), https://github.com/elasticdotventures/_b00t_/issues/1177#issuecomment-5467568144 (the new P9 milestone note on the parent spine epic)

## Global Constraints

- **Honest divergence from event-dispatched state machines, documented in code.** `FoundryLocalLifecycle` has no live "engine" driving transitions the way `OodaStateMachine::dispatch` does — there's no explicit event stream anywhere in this codebase for Foundry Local's install/service state. `current_state(&FoundryLocalStatus) -> FoundryLocalLifecycle` is a pure derivation from a status snapshot, computed fresh each time `FoundryLocalStatus` is collected. This is the real source of truth this phase adds; the SCXML export models the *shape* of how that derived state can move over time (useful for visualization/governance, per the epic owner's own framing), not a literal transition log. Document this explicitly in the module doc comment, mirroring how `ufo-types::statechart`'s own doc comment flags its `Cancel`-as-`Final`-state divergence from the live OODA dispatcher.
- Add `scxml = "=0.2.2"` as an **unconditional** dependency to `ledgerr-desktop-agent/Cargo.toml` — not feature-gated. `ufo-types` gates its own `scxml` dependency behind a `statechart` feature because it serves many different consumers with different needs (including wasm32 targets); `ledgerr-desktop-agent` is a single-purpose Windows desktop controller crate with no such constraint, and status reporting is already a core, always-used capability of this crate — gating adds complexity with no compensating benefit here.
- Follow `ufo-types::statechart::ooda_phases_to_statechart`'s exact API shape: `scxml::model::{State, Statechart, Transition}`, `State::atomic(id)` for non-terminal states, `Transition::new(event, target)`, `Statechart::new(initial_state_id, states).with_name(name)`. Test with the same four-test shape that module uses: state-count/presence, a specific-transition-exists check, `scxml::validate(&chart)` structural validation, and `scxml::export::xml::to_xml` / `scxml::parse_xml` round-trip equality.
- Work happens in the existing worktree at `/home/brianh/promptexecution/_b00t_/vendor/ledgrrr` (WSL, git source of truth), branch `feat/tray-client-server-port` (PR #216, open) — this plan's commits land ON TOP of it as new commits on the same branch (stacked), continuing this session's established pattern. Mirror every edit to `/mnt/d/promptjects/ledgrrr` before running `cargo` (WSL has no C linker at all): `cp crates/ledgerr-desktop-agent/src/lifecycle.rs /mnt/d/promptjects/ledgrrr/crates/ledgerr-desktop-agent/src/lifecycle.rs` (and separately for `lib.rs`/`Cargo.toml`). **Before starting Task 1, verify the D: mirror is in sync**: `diff -rq crates/ledgerr-desktop-agent /mnt/d/promptjects/ledgrrr/crates/ledgerr-desktop-agent` — it was confirmed in sync as of Phase 4's completion; if anything has drifted since, sync FROM the WSL worktree (authoritative) first.
- Always pass `-j 2` to `cargo build`/`cargo test` on the D: side, scoped to `-p ledgerr-desktop-agent`: `pwsh.exe -NoProfile -Command "cd D:\promptjects\ledgrrr; cargo test -p ledgerr-desktop-agent -j 2 <filter> -- --nocapture"`.
- A known, already-established pre-existing test failure exists in this crate (`tests/contract.rs::repair_plan_uses_cached_test_package_when_present`, plus a lock-poisoning cascade onto `export_office_artifact_bumps_version_on_repeat_export_and_never_overwrites`) — confirmed unrelated to any Foundry Local epic work, root-caused to a missing test fixture on the D: build machine. Expect the crate's full test suite to show these 2 pre-existing failures; do not investigate them again, and do not let them block this plan's own new tests from being verified individually.

---

### Task 1: `FoundryLocalLifecycle` enum + status derivation

**Files:**
- Create: `crates/ledgerr-desktop-agent/src/lifecycle.rs`
- Modify: `crates/ledgerr-desktop-agent/src/lib.rs` (module declaration)
- Modify: `crates/ledgerr-desktop-agent/Cargo.toml` (add `scxml` dependency — needed by Task 2, but adding it here keeps both tasks' `Cargo.toml` needs in one place; Task 1 itself doesn't use `scxml` yet)

**Interfaces:**
- Consumes: `crate::status::FoundryLocalStatus` (Phase 4, already real — `pub struct FoundryLocalStatus { pub cli_found: bool, pub service_running: bool }`).
- Produces: `pub enum FoundryLocalLifecycle { NotInstalled, InstalledStopped, InstalledRunning }` (derive `Debug, Clone, Copy, PartialEq, Eq`), `pub fn current_state(status: &FoundryLocalStatus) -> FoundryLocalLifecycle`, `pub(crate) fn state_id(state: FoundryLocalLifecycle) -> &'static str` (returns `"not_installed"`/`"installed_stopped"`/`"installed_running"` — `pub(crate)` since Task 2 in the same file needs it, no external consumer yet). Consumed by Task 2 in this same file.

- [ ] **Step 1: Add the scxml dependency**

In `crates/ledgerr-desktop-agent/Cargo.toml`'s `[dependencies]` section, add:

```toml
scxml = "=0.2.2"
```

- [ ] **Step 2: Write the failing test**

Create `crates/ledgerr-desktop-agent/src/lifecycle.rs`:

```rust
//! Windows Foundry Local's install/service lifecycle, and its export as a
//! W3C SCXML statechart (`foundry_lifecycle_to_statechart`, added in a
//! later step of this module) — following the exact pattern
//! `ufo-types::statechart::ooda_phases_to_statechart` already establishes
//! for this ecosystem's SysML-v2/statechart spine
//! (`elasticdotventures/_b00t_#1177`).
//!
//! Deliberate divergence from an event-dispatched state machine (e.g.
//! `OodaStateMachine::dispatch`): there is no live "engine" driving
//! transitions here. [`current_state`] is a pure derivation from a
//! [`crate::status::FoundryLocalStatus`] snapshot, computed fresh every
//! time status is collected — that function IS this phase's real source of
//! truth. The SCXML export models the *shape* of how that derived state
//! can move over time (useful for visualization/governance), not a
//! literal transition log; this mirrors how `ooda_phases_to_statechart`'s
//! own doc comment flags its `Cancel`-as-`Final`-state divergence from the
//! live OODA dispatcher.

use crate::status::FoundryLocalStatus;

/// Windows Foundry Local's observed lifecycle state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FoundryLocalLifecycle {
    /// The `foundry` CLI is not on PATH.
    NotInstalled,
    /// The CLI is present but its service is not reporting as running.
    InstalledStopped,
    /// The CLI is present and its service reports as running.
    InstalledRunning,
}

/// Derives the current lifecycle state from a real status snapshot. This
/// is the authoritative logic — the SCXML export (see
/// `foundry_lifecycle_to_statechart`, added later in this file) documents
/// this same shape for visualization, it does not replace this function.
pub fn current_state(status: &FoundryLocalStatus) -> FoundryLocalLifecycle {
    if !status.cli_found {
        FoundryLocalLifecycle::NotInstalled
    } else if status.service_running {
        FoundryLocalLifecycle::InstalledRunning
    } else {
        FoundryLocalLifecycle::InstalledStopped
    }
}

pub(crate) fn state_id(state: FoundryLocalLifecycle) -> &'static str {
    match state {
        FoundryLocalLifecycle::NotInstalled => "not_installed",
        FoundryLocalLifecycle::InstalledStopped => "installed_stopped",
        FoundryLocalLifecycle::InstalledRunning => "installed_running",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cli_not_found_means_not_installed_regardless_of_service_flag() {
        let status = FoundryLocalStatus {
            cli_found: false,
            service_running: true, // must be ignored — cli_found gates everything
        };
        assert_eq!(current_state(&status), FoundryLocalLifecycle::NotInstalled);
    }

    #[test]
    fn cli_found_and_service_not_running_is_installed_stopped() {
        let status = FoundryLocalStatus {
            cli_found: true,
            service_running: false,
        };
        assert_eq!(current_state(&status), FoundryLocalLifecycle::InstalledStopped);
    }

    #[test]
    fn cli_found_and_service_running_is_installed_running() {
        let status = FoundryLocalStatus {
            cli_found: true,
            service_running: true,
        };
        assert_eq!(current_state(&status), FoundryLocalLifecycle::InstalledRunning);
    }

    #[test]
    fn state_ids_are_distinct_and_stable() {
        assert_eq!(state_id(FoundryLocalLifecycle::NotInstalled), "not_installed");
        assert_eq!(state_id(FoundryLocalLifecycle::InstalledStopped), "installed_stopped");
        assert_eq!(state_id(FoundryLocalLifecycle::InstalledRunning), "installed_running");
    }
}
```

- [ ] **Step 3: Run test to verify it fails**

Sync the new file (`cp crates/ledgerr-desktop-agent/src/lifecycle.rs /mnt/d/promptjects/ledgrrr/crates/ledgerr-desktop-agent/src/lifecycle.rs`), then:
Run: `pwsh.exe -NoProfile -Command "cd D:\promptjects\ledgrrr; cargo test -p ledgerr-desktop-agent -j 2 lifecycle:: -- --nocapture"`
Expected: FAIL — module not found (not wired into `lib.rs` yet).

- [ ] **Step 4: Wire the module into lib.rs**

In `crates/ledgerr-desktop-agent/src/lib.rs`, add (alphabetically — after `pub mod install_plan;`, before `pub mod office_artifact;`, since `install_plan` < `lifecycle` < `office_artifact`):

```rust
pub mod lifecycle;
```

- [ ] **Step 5: Run test to verify it passes**

Sync `lib.rs` too, then:
Run: `pwsh.exe -NoProfile -Command "cd D:\promptjects\ledgrrr; cargo test -p ledgerr-desktop-agent -j 2 lifecycle:: -- --nocapture"`
Expected: PASS (4 tests)

- [ ] **Step 6: Run the crate's full test suite**

Run: `pwsh.exe -NoProfile -Command "cd D:\promptjects\ledgrrr; cargo test -p ledgerr-desktop-agent -j 2"`
Expected: the 2 known pre-existing failures (`repair_plan_uses_cached_test_package_when_present` and its lock-poisoning cascade), everything else green including the 4 new tests.

- [ ] **Step 7: Commit**

```bash
cd /home/brianh/promptexecution/_b00t_/vendor/ledgrrr
git add crates/ledgerr-desktop-agent/Cargo.toml crates/ledgerr-desktop-agent/src/lifecycle.rs crates/ledgerr-desktop-agent/src/lib.rs
git commit -m "feat(lifecycle): add FoundryLocalLifecycle enum and status derivation"
```

---

### Task 2: SCXML statechart export

**Files:**
- Modify: `crates/ledgerr-desktop-agent/src/lifecycle.rs` (add export function + tests, same file as Task 1)

**Interfaces:**
- Consumes: `FoundryLocalLifecycle`, `state_id` (Task 1, same file).
- Produces: `pub fn foundry_lifecycle_to_statechart() -> scxml::model::Statechart`.

- [ ] **Step 1: Write the failing test**

Add to `crates/ledgerr-desktop-agent/src/lifecycle.rs`'s `#[cfg(test)] mod tests` block (after `state_ids_are_distinct_and_stable`):

```rust
    use scxml::export::xml::to_xml;
    use scxml::model::StateKind;
    use scxml::parse_xml;
    use scxml::validate;

    #[test]
    fn statechart_has_exactly_the_three_lifecycle_states() {
        let chart = foundry_lifecycle_to_statechart();
        assert_eq!(chart.states.len(), 3);
        for state in [
            FoundryLocalLifecycle::NotInstalled,
            FoundryLocalLifecycle::InstalledStopped,
            FoundryLocalLifecycle::InstalledRunning,
        ] {
            let found = chart.find_state(state_id(state));
            assert!(found.is_some(), "missing state {}", state_id(state));
            assert_eq!(found.unwrap().kind, StateKind::Atomic);
        }
    }

    #[test]
    fn not_installed_transitions_to_installed_stopped_on_install_succeeded() {
        let chart = foundry_lifecycle_to_statechart();
        let not_installed = chart
            .find_state(state_id(FoundryLocalLifecycle::NotInstalled))
            .unwrap();
        let transition = not_installed
            .transitions
            .iter()
            .find(|t| t.event.as_deref() == Some("InstallSucceeded"))
            .expect("NotInstalled must accept InstallSucceeded");
        assert_eq!(
            transition.targets,
            vec![state_id(FoundryLocalLifecycle::InstalledStopped)]
        );
    }

    #[test]
    fn installed_stopped_and_running_transition_to_each_other() {
        let chart = foundry_lifecycle_to_statechart();
        let stopped = chart
            .find_state(state_id(FoundryLocalLifecycle::InstalledStopped))
            .unwrap();
        let start = stopped
            .transitions
            .iter()
            .find(|t| t.event.as_deref() == Some("ServiceStarted"))
            .expect("InstalledStopped must accept ServiceStarted");
        assert_eq!(start.targets, vec![state_id(FoundryLocalLifecycle::InstalledRunning)]);

        let running = chart
            .find_state(state_id(FoundryLocalLifecycle::InstalledRunning))
            .unwrap();
        let stop = running
            .transitions
            .iter()
            .find(|t| t.event.as_deref() == Some("ServiceStopped"))
            .expect("InstalledRunning must accept ServiceStopped");
        assert_eq!(stop.targets, vec![state_id(FoundryLocalLifecycle::InstalledStopped)]);
    }

    #[test]
    fn statechart_passes_scxml_structural_validation() {
        let chart = foundry_lifecycle_to_statechart();
        validate(&chart).expect("exported statechart should be structurally valid SCXML");
    }

    #[test]
    fn statechart_round_trips_through_xml_export_and_parse() {
        let chart = foundry_lifecycle_to_statechart();
        let xml = to_xml(&chart);
        let parsed = parse_xml(&xml).expect("exported XML must parse back as valid SCXML");
        assert_eq!(parsed, chart);
    }
```

Note: this adds `use` statements inside the `tests` module — if Step 2 below (adding `use scxml::model::{State, Statechart, Transition};` at the top of the file, outside `tests`) already brings some of these into scope at module level, avoid a duplicate/conflicting import; keep the test-only imports (`to_xml`, `StateKind`, `parse_xml`, `validate`) inside `mod tests` as shown, since the non-test code only needs `State`, `Statechart`, `Transition`.

- [ ] **Step 2: Run test to verify it fails**

Sync the file, then:
Run: `pwsh.exe -NoProfile -Command "cd D:\promptjects\ledgrrr; cargo test -p ledgerr-desktop-agent -j 2 lifecycle:: -- --nocapture"`
Expected: FAIL to compile — `foundry_lifecycle_to_statechart` not found.

- [ ] **Step 3: Write the minimal implementation**

Add to `crates/ledgerr-desktop-agent/src/lifecycle.rs`, below `state_id`, above the `#[cfg(test)]` module. First add the import at the top of the file (with the existing `use crate::status::FoundryLocalStatus;` line):

```rust
use scxml::model::{State, Statechart, Transition};
```

Then the export function:

```rust
/// Exports [`FoundryLocalLifecycle`]'s state shape as a W3C SCXML
/// statechart, following `ufo-types::statechart::ooda_phases_to_statechart`'s
/// exact pattern. See this module's doc comment for why these transitions
/// describe the *shape* of the lifecycle rather than a literally-dispatched
/// event stream.
pub fn foundry_lifecycle_to_statechart() -> Statechart {
    let mut not_installed = State::atomic(state_id(FoundryLocalLifecycle::NotInstalled));
    not_installed.transitions.push(Transition::new(
        "InstallSucceeded",
        state_id(FoundryLocalLifecycle::InstalledStopped),
    ));

    let mut installed_stopped = State::atomic(state_id(FoundryLocalLifecycle::InstalledStopped));
    installed_stopped.transitions.push(Transition::new(
        "ServiceStarted",
        state_id(FoundryLocalLifecycle::InstalledRunning),
    ));

    let mut installed_running = State::atomic(state_id(FoundryLocalLifecycle::InstalledRunning));
    installed_running.transitions.push(Transition::new(
        "ServiceStopped",
        state_id(FoundryLocalLifecycle::InstalledStopped),
    ));

    Statechart::new(
        state_id(FoundryLocalLifecycle::NotInstalled),
        vec![not_installed, installed_stopped, installed_running],
    )
    .with_name("foundry_local_lifecycle")
}
```

- [ ] **Step 4: Run test to verify it passes**

Sync the file, then:
Run: `pwsh.exe -NoProfile -Command "cd D:\promptjects\ledgrrr; cargo test -p ledgerr-desktop-agent -j 2 lifecycle:: -- --nocapture"`
Expected: PASS (9 tests total in this module: 4 from Task 1 + 5 new)

- [ ] **Step 5: Run the crate's full test suite**

Run: `pwsh.exe -NoProfile -Command "cd D:\promptjects\ledgrrr; cargo test -p ledgerr-desktop-agent -j 2"`
Expected: same 2 known pre-existing failures, everything else green.

- [ ] **Step 6: Commit**

```bash
cd /home/brianh/promptexecution/_b00t_/vendor/ledgrrr
git add crates/ledgerr-desktop-agent/src/lifecycle.rs
git commit -m "feat(lifecycle): export FoundryLocalLifecycle as an SCXML statechart"
```

---

## Final Verification Checklist (for the validating/committing agent)

Run these against `/mnt/d/promptjects/ledgrrr` (via `pwsh.exe`) for `cargo`, and the WSL worktree `/home/brianh/promptexecution/_b00t_/vendor/ledgrrr` for `git`, after both tasks are complete, before pushing:

- [ ] `pwsh.exe -NoProfile -Command "cd D:\promptjects\ledgrrr; cargo test -p ledgerr-desktop-agent -j 2"` — 2 known pre-existing failures (`repair_plan_uses_cached_test_package_when_present` and its cascade), everything else passes including the 9 new `lifecycle::` tests.
- [ ] `pwsh.exe -NoProfile -Command "cd D:\promptjects\ledgrrr; cargo build -p ledgerr-host --bin host-tauri -j 2"` — no errors (confirms `ledgerr-host`, which depends on `ledgerr-desktop-agent`, still builds cleanly with the new `scxml` dependency in the graph).
- [ ] Confirm the WSL worktree and D: mirror are in sync for every file this plan touched (`diff` each of `Cargo.toml`, `lifecycle.rs`, `lib.rs`).
- [ ] This plan's commits land on the EXISTING branch `feat/tray-client-server-port` (PR #216) — do NOT create a new branch, do NOT open a new PR. Push with `git push`, report the updated PR URL (still #216), confirm via `git log --oneline -8` and (per this session's established practice) `git ls-remote origin feat/tray-client-server-port` to verify against GitHub directly rather than trusting a possibly-stale local tracking ref.
- [ ] After the push succeeds, post a completion comment on `PromptExecution/ledgrrr#220` and on `elasticdotventures/_b00t_#1177` (P9's completion, matching that epic's own established "status update" comment convention) — this is the last phase of the epic; the comment should note the epic is now fully shipped as PRs across `ufo-types`, `_b00t_`, and `ledgrrr` (Phase 1: `ufo-types#12`, Phase 2: `_b00t_#1200`, Phase 3: `ufo-types#14`, Phase 4+5: `ledgrrr#216`), all open and awaiting review/merge by the repo owner.
