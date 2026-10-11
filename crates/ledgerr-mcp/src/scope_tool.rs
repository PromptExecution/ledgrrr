//! `ledgerr_scope` — runtime identity and scope-increase tool (issue #226).
//!
//! Provides two actions:
//! - `whoami` — returns the caller's current identity, ring, and visible tool families.
//! - `request_increase` — asks for a ring promotion evaluated through `LedgrrAgtGateway`.

use msft_agent_gov_ledgrrr::{rings, LedgrrAgtGateway, Ring};
use serde_json::{json, Value};

/// `whoami` response — what the caller can see and at what ring.
pub struct ScopeWhoami {
    pub agent_id: Option<String>,
    pub ring: String,
    pub granted_families: Vec<String>,
    pub core_families: Vec<String>,
}

impl ScopeWhoami {
    pub fn into_value(self) -> Value {
        json!({
            "agent_id": self.agent_id,
            "ring": self.ring,
            "granted_families": self.granted_families,
            "core_families": self.core_families,
        })
    }
}

/// Result of a `request_increase` call.
pub struct ScopeIncreaseResult {
    pub granted: bool,
    pub new_ring: String,
    pub requires_approval: bool,
    pub reason: Option<String>,
    /// True when the ring actually changed — caller should send `notifications/tools/list_changed`.
    pub list_changed: bool,
}

impl ScopeIncreaseResult {
    pub fn into_value(self) -> Value {
        json!({
            "granted": self.granted,
            "new_ring": self.new_ring,
            "requires_approval": self.requires_approval,
            "reason": self.reason,
        })
    }
}

/// Handle `ledgerr_scope.whoami` — returns identity + ring + visible families.
pub fn handle_whoami(agent_id: Option<&str>, ring: Option<Ring>) -> ScopeWhoami {
    let ring_str = ring
        .map(ring_name)
        .unwrap_or_else(|| "unfiltered".to_string());

    let granted_families: Vec<String> = if let Some(r) = ring {
        if r == Ring::Admin {
            // Admin sees everything — return all ring-gated families from policy.
            msft_agent_gov_ledgrrr::policy::PUBLISHED_TOOL_NAMES
                .iter()
                .map(|s| s.to_string())
                .collect()
        } else {
            rings::ring_visible_tool_families(r)
                .into_iter()
                .collect()
        }
    } else {
        // No ring configured → all families visible.
        msft_agent_gov_ledgrrr::policy::PUBLISHED_TOOL_NAMES
            .iter()
            .map(|s| s.to_string())
            .collect()
    };

    let core_families: Vec<String> = rings::CORE_TOOL_FAMILIES
        .iter()
        .map(|s| s.to_string())
        .collect();

    ScopeWhoami {
        agent_id: agent_id.map(String::from),
        ring: ring_str,
        granted_families,
        core_families,
    }
}

/// Handle `ledgerr_scope.request_increase` — evaluate via gateway, return decision.
///
/// `target_ring` is one of: `"admin"`, `"standard"`, `"restricted"`.
/// On auto-grant, calls `register_agent_at_ring` / `promote_to_admin` immediately.
/// Returns `ScopeIncreaseResult` with `list_changed = true` if the ring changed.
pub fn handle_request_increase(
    agent_id: Option<&str>,
    current_ring: Option<Ring>,
    target_ring_str: &str,
    gw: &LedgrrAgtGateway,
) -> ScopeIncreaseResult {
    let Some(agent_id) = agent_id else {
        return ScopeIncreaseResult {
            granted: false,
            new_ring: "unknown".to_string(),
            requires_approval: false,
            reason: Some(
                "no agent identity configured; set LEDGERR_MCP_AGENT_ID or X-Agent-Id".to_string(),
            ),
            list_changed: false,
        };
    };

    let Some(target) = parse_ring(target_ring_str) else {
        return ScopeIncreaseResult {
            granted: false,
            new_ring: current_ring.map(ring_name).unwrap_or_else(|| "unknown".to_string()),
            requires_approval: false,
            reason: Some(format!(
                "unknown target_ring '{target_ring_str}'; valid: admin, standard, restricted"
            )),
            list_changed: false,
        };
    };

    let action = format!("request_increase.{}", ring_name(target));
    let decision = gw.check_tool_call(agent_id, "ledgerr_scope", &action);

    if !decision.allowed {
        // decision.reason already encodes the policy variant:
        // "approval_required: ..." for RequiresApproval, otherwise a deny reason.
        let reason_str = decision.reason.clone().unwrap_or_default();
        let requires_approval = reason_str.starts_with("approval_required:");
        return ScopeIncreaseResult {
            granted: false,
            new_ring: current_ring.map(ring_name).unwrap_or_else(|| "unknown".to_string()),
            requires_approval,
            reason: Some(reason_str),
            list_changed: false,
        };
    }

    // Auto-granted: perform the ring change.
    let new_ring = match target {
        Ring::Admin => {
            let _ = gw.promote_to_admin(agent_id);
            Ring::Admin
        }
        r => {
            gw.register_agent_at_ring(agent_id, r)
                .unwrap_or(current_ring.unwrap_or(Ring::Standard))
        }
    };

    let changed = current_ring != Some(new_ring);
    ScopeIncreaseResult {
        granted: true,
        new_ring: ring_name(new_ring),
        requires_approval: false,
        reason: None,
        list_changed: changed,
    }
}

/// Dispatch `ledgerr_scope` tool calls. Returns `(result_value, list_changed)`.
///
/// `list_changed` is true when a `request_increase` succeeded and the ring changed —
/// the caller should emit `notifications/tools/list_changed` after writing the result.
pub fn dispatch(
    agent_id: Option<&str>,
    ring: Option<Ring>,
    arguments: &Value,
    gw: &LedgrrAgtGateway,
) -> (Value, bool) {
    let action = arguments
        .get("action")
        .and_then(Value::as_str)
        .unwrap_or("");

    match action {
        "whoami" => {
            let whoami = handle_whoami(agent_id, ring);
            (
                json!({
                    "content": [{"type": "text", "text": serde_json::to_string_pretty(&whoami.into_value()).unwrap_or_default()}],
                    "isError": false
                }),
                false,
            )
        }
        "request_increase" => {
            let target_ring = arguments
                .get("target_ring")
                .and_then(Value::as_str)
                .unwrap_or("");
            let result = handle_request_increase(agent_id, ring, target_ring, gw);
            let list_changed = result.list_changed;
            (
                json!({
                    "content": [{"type": "text", "text": serde_json::to_string_pretty(&result.into_value()).unwrap_or_default()}],
                    "isError": false
                }),
                list_changed,
            )
        }
        _ => (
            json!({
                "content": [{"type": "text", "text": format!("unknown action '{}'; valid: whoami, request_increase", action)}],
                "isError": true
            }),
            false,
        ),
    }
}

fn ring_name(ring: Ring) -> String {
    match ring {
        Ring::Admin => "admin".to_string(),
        Ring::Standard => "standard".to_string(),
        Ring::Restricted => "restricted".to_string(),
        Ring::Sandboxed => "sandboxed".to_string(),
    }
}

fn parse_ring(s: &str) -> Option<Ring> {
    match s.trim().to_ascii_lowercase().as_str() {
        "admin" => Some(Ring::Admin),
        "standard" => Some(Ring::Standard),
        "restricted" => Some(Ring::Restricted),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn whoami_no_ring_returns_all_families() {
        let result = handle_whoami(Some("test-agent"), None);
        assert_eq!(result.agent_id, Some("test-agent".to_string()));
        assert_eq!(result.ring, "unfiltered");
        assert!(!result.granted_families.is_empty());
        assert!(result.core_families.contains(&"ledgerr_schema".to_string()));
    }

    #[test]
    fn whoami_standard_ring_excludes_reconciliation() {
        let result = handle_whoami(Some("test-agent"), Some(Ring::Standard));
        assert_eq!(result.ring, "standard");
        assert!(!result.granted_families.contains(&"ledgerr_reconciliation".to_string()));
        assert!(result.granted_families.contains(&"ledgerr_documents".to_string()));
    }

    #[test]
    fn whoami_no_agent_id() {
        let result = handle_whoami(None, Some(Ring::Restricted));
        assert_eq!(result.agent_id, None);
        assert_eq!(result.ring, "restricted");
    }

    #[test]
    fn request_increase_no_identity_returns_not_granted() {
        let gw = msft_agent_gov_ledgrrr::LedgrrAgtGateway::new("ledgerr-mcp-server")
            .expect("gateway");
        let result = handle_request_increase(None, None, "standard", &gw);
        assert!(!result.granted);
        assert!(result.reason.is_some());
    }

    #[test]
    fn request_increase_unknown_target_ring_returns_not_granted() {
        let gw = msft_agent_gov_ledgrrr::LedgrrAgtGateway::new("ledgerr-mcp-server")
            .expect("gateway");
        let result = handle_request_increase(Some("agent-1"), None, "superuser", &gw);
        assert!(!result.granted);
        assert!(result.reason.unwrap().contains("unknown target_ring"));
    }
}
