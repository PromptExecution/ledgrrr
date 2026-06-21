//! Visual observer — fires a CDP screenshot + vision analysis script against
//! a running Tauri WebView2 session and returns structured observations.
//!
//! Intended for use in the `kaizen` build→test loop. If the CDP endpoint is
//! unreachable the observer returns a graceful `CDP_UNAVAILABLE` result
//! rather than failing — this lets tests run without a live Tauri instance.

use crate::HolonError;
use std::path::PathBuf;
use std::time::Duration;

/// Structured result of one visual observation round.
#[derive(Debug, Clone)]
pub struct VizObservation {
    /// Path where the CDP screenshot was saved (may not exist on CDP_UNAVAILABLE).
    pub screenshot_path: PathBuf,
    /// Number of Cytoscape nodes visible in the screenshot (0 if unavailable).
    pub node_count: usize,
    /// Number of Cytoscape edges visible in the screenshot (0 if unavailable).
    pub edge_count: usize,
    /// Raw stdout / status from the vision script, or "CDP_UNAVAILABLE".
    pub raw_output: String,
}

impl VizObservation {
    /// Returns true when the observation successfully connected to CDP.
    pub fn is_live(&self) -> bool {
        self.raw_output != "CDP_UNAVAILABLE"
    }
}

/// Fires CDP screenshot + vision analysis against a Tauri WebView2 session.
pub struct VizObserver {
    /// Where the CDP screenshot should be saved locally.
    pub screenshot_path: PathBuf,
    /// Path to `scripts/tauri-vision-analyze.py`.
    pub vision_script: PathBuf,
    /// CDP DevTools URL (default: `http://localhost:19222`).
    pub cdp_url: String,
    /// Maximum time to wait for the vision script (default: 30 s).
    pub timeout: Duration,
}

impl VizObserver {
    /// Construct with defaults for the standard Tauri WebView2 CDP port.
    pub fn new(screenshot_path: PathBuf, vision_script: PathBuf) -> Self {
        Self {
            screenshot_path,
            vision_script,
            cdp_url: "http://localhost:19222".to_string(),
            timeout: Duration::from_secs(30),
        }
    }

    /// Run the vision script against the live CDP session.
    ///
    /// Returns `Ok(VizObservation)` in all cases — a CDP connection failure
    /// is surfaced as `raw_output == "CDP_UNAVAILABLE"` rather than an `Err`.
    /// Only returns `Err` for unrecoverable internal errors (e.g. JSON parse
    /// of a well-formed but structurally unexpected observation file).
    pub fn observe(&self) -> Result<VizObservation, HolonError> {
        let obs_json_path = std::env::temp_dir().join("holon-viz-obs.json");

        // Remove stale output from prior runs.
        let _ = std::fs::remove_file(&obs_json_path);

        let output = std::process::Command::new("python3")
            .arg(&self.vision_script)
            .arg("--cdp")
            .arg(&self.cdp_url)
            .arg("--screenshot")
            .arg(&self.screenshot_path)
            .arg("--output-json")
            .arg(&obs_json_path)
            .output();

        let output = match output {
            Ok(o) if o.status.success() => o,
            _ => {
                return Ok(VizObservation {
                    screenshot_path: self.screenshot_path.clone(),
                    node_count: 0,
                    edge_count: 0,
                    raw_output: "CDP_UNAVAILABLE".to_string(),
                });
            }
        };

        let raw_output = String::from_utf8_lossy(&output.stdout).into_owned();

        // Parse optional structured JSON output.
        let (node_count, edge_count) = Self::parse_obs_json(&obs_json_path);

        Ok(VizObservation {
            screenshot_path: self.screenshot_path.clone(),
            node_count,
            edge_count,
            raw_output,
        })
    }

    fn parse_obs_json(path: &std::path::Path) -> (usize, usize) {
        let Ok(text) = std::fs::read_to_string(path) else {
            return (0, 0);
        };
        let Ok(val) = serde_json::from_str::<serde_json::Value>(&text) else {
            return (0, 0);
        };
        let nodes = val["node_count"].as_u64().unwrap_or(0) as usize;
        let edges = val["edge_count"].as_u64().unwrap_or(0) as usize;
        (nodes, edges)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn observe_returns_cdp_unavailable_when_no_tauri() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let screenshot = tmp.path().join("shot.png");
        // Point at a non-existent script so python3 fails immediately.
        let script = tmp.path().join("nonexistent.py");
        let observer = VizObserver::new(screenshot.clone(), script);
        let obs = observer.observe().expect("observe should not Err");
        assert_eq!(obs.raw_output, "CDP_UNAVAILABLE");
        assert_eq!(obs.node_count, 0);
        assert_eq!(obs.edge_count, 0);
        assert!(!obs.is_live());
    }

    #[test]
    fn viz_observation_is_live_false_on_unavailable() {
        let obs = VizObservation {
            screenshot_path: PathBuf::from("/tmp/shot.png"),
            node_count: 0,
            edge_count: 0,
            raw_output: "CDP_UNAVAILABLE".to_string(),
        };
        assert!(!obs.is_live());
    }

    #[test]
    fn viz_observation_is_live_true_on_real_output() {
        let obs = VizObservation {
            screenshot_path: PathBuf::from("/tmp/shot.png"),
            node_count: 4,
            edge_count: 3,
            raw_output: "ok\nnodes=4 edges=3\n".to_string(),
        };
        assert!(obs.is_live());
    }
}
