use std::sync::{Arc, Mutex};

use ledgerr_host::chat::{ChatTurn, ReviewLog};
use ledgerr_host::evidence::EvidenceState;
use ledgerr_host::internal_openai::InternalOpenAiHandle;

use ledgerr_host::settings_client::SettingsClient;

pub struct AppState {
    pub store: Arc<SettingsClient>,
    pub history: Arc<Mutex<Vec<ChatTurn>>>,
    pub review_log: Arc<Mutex<ReviewLog>>,
    pub internal_endpoint: Arc<Mutex<Option<InternalOpenAiHandle>>>,
    pub evidence: Arc<Mutex<EvidenceState>>,
}
