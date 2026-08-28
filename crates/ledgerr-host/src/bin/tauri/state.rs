use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use ledgerr_host::chat::{ChatTurn, ReviewLog, SuspendedToolLoop};
use ledgerr_host::evidence::EvidenceState;
use ledgerr_host::internal_openai::InternalOpenAiHandle;

use ledgerr_host::settings_client::SettingsClient;

/// A tool-calling loop suspended awaiting operator confirmation of one or
/// more mutation tool calls, plus the resolutions recorded so far.
///
/// Only one confirmation round can be in flight at a time — this desktop app
/// has a single chat session, so a single `Option` slot in `AppState` is
/// enough; a second `send_message` while a confirmation is pending would
/// need to wait for/replace it, same as any other single-flight chat turn.
pub struct PendingToolLoopSession {
    pub suspended: SuspendedToolLoop,
    /// call id -> approved. Populated incrementally as
    /// `confirm_pending_tool_call` is invoked once per pending id.
    pub resolutions: HashMap<String, bool>,
}

pub struct AppState {
    pub store: Arc<SettingsClient>,
    pub history: Arc<Mutex<Vec<ChatTurn>>>,
    pub review_log: Arc<Mutex<ReviewLog>>,
    pub internal_endpoint: Arc<Mutex<Option<InternalOpenAiHandle>>>,
    pub evidence: Arc<Mutex<EvidenceState>>,
    pub pending_tool_loop: Arc<Mutex<Option<PendingToolLoopSession>>>,
}
