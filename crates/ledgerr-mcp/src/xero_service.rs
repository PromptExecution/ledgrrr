/// Xero integration service layer — bridges MCP dispatch to ledgerr-xero.
#[cfg(feature = "xero")]
use std::collections::BTreeMap;
#[cfg(feature = "xero")]
use std::path::{Path, PathBuf};
#[cfg(feature = "xero")]
use std::sync::Mutex;

#[cfg(feature = "xero")]
use ledgerr_xero::{XeroClient, XeroConfig, XeroEntityRef};
#[cfg(feature = "xero")]
use serde_json::{json, Value};
#[cfg(feature = "xero")]
use tracing::info;

#[cfg(feature = "xero")]
use crate::{
    ontology::{OntologyEntityInput, OntologyEntityKind, OntologyStore},
    ToolError,
};

#[cfg(feature = "xero")]
pub struct XeroService {
    /// The mutable client is kept behind a Mutex so the service can be shared
    /// via `&TurboLedgerService` (which is `Sync`). All Xero calls are blocking.
    client: Mutex<Option<XeroClient>>,
    config: XeroConfig,
    token_path: PathBuf,
}

#[cfg(feature = "xero")]
impl XeroService {
    pub fn new(config: XeroConfig, token_path: PathBuf) -> Self {
        // Try to load persisted tokens; silently ignore if unavailable.
        let client = XeroClient::new(config.clone(), token_path.clone()).ok();
        Self {
            client: Mutex::new(client),
            config,
            token_path,
        }
    }

    pub fn is_authenticated(&self) -> bool {
        self.client
            .lock()
            .ok()
            .and_then(|g| g.as_ref().map(|c| c.is_authenticated()))
            .unwrap_or(false)
    }

    pub fn get_auth_url(&self) -> Result<String, ToolError> {
        let config = self.config.clone();
        let mut new_client = XeroClient::new(config, self.token_path.clone())
            .map_err(|e| ToolError::Internal(e.to_string()))?;
        let url = new_client
            .get_auth_url()
            .map_err(|e| ToolError::Internal(e.to_string()))?;

        // Store the client so the pending PKCE state is available for exchange_code().
        let mut guard = self
            .client
            .lock()
            .map_err(|_| ToolError::Internal("lock poisoned".into()))?;
        *guard = Some(new_client);

        Ok(url)
    }

    pub fn exchange_code(&self, code: String, state: String) -> Result<Value, ToolError> {
        let mut guard = self
            .client
            .lock()
            .map_err(|_| ToolError::Internal("lock poisoned".into()))?;
        let client = guard.as_mut().ok_or_else(|| {
            ToolError::Internal("No pending auth flow; call get_auth_url first".into())
        })?;
        let tenant = client
            .exchange_code(code, state)
            .map_err(|e| ToolError::Internal(e.to_string()))?;

        info!(tenant_id = %tenant.tenant_id, "Xero authenticated");
        Ok(json!({
            "authenticated": true,
            "tenant_id": tenant.tenant_id,
            "tenant_name": tenant.tenant_name,
        }))
    }

    pub fn fetch_contacts(&self, search: Option<&str>) -> Result<Vec<XeroEntityRef>, ToolError> {
        let mut guard = self
            .client
            .lock()
            .map_err(|_| ToolError::Internal("lock poisoned".into()))?;
        let client = guard
            .as_mut()
            .ok_or_else(|| ToolError::Internal("Not authenticated with Xero".into()))?;

        let contacts = match search {
            Some(q) => client.search_contacts(q),
            None => client.get_contacts(),
        }
        .map_err(|e| ToolError::Internal(e.to_string()))?;

        Ok(contacts.iter().map(XeroEntityRef::from).collect())
    }

    pub fn fetch_accounts(&self) -> Result<Vec<XeroEntityRef>, ToolError> {
        let mut guard = self
            .client
            .lock()
            .map_err(|_| ToolError::Internal("lock poisoned".into()))?;
        let client = guard
            .as_mut()
            .ok_or_else(|| ToolError::Internal("Not authenticated with Xero".into()))?;
        let accounts = client
            .get_accounts()
            .map_err(|e| ToolError::Internal(e.to_string()))?;
        Ok(accounts.iter().map(XeroEntityRef::from).collect())
    }

    pub fn fetch_bank_accounts(&self) -> Result<Vec<XeroEntityRef>, ToolError> {
        let mut guard = self
            .client
            .lock()
            .map_err(|_| ToolError::Internal("lock poisoned".into()))?;
        let client = guard
            .as_mut()
            .ok_or_else(|| ToolError::Internal("Not authenticated with Xero".into()))?;
        let accounts = client
            .get_bank_accounts()
            .map_err(|e| ToolError::Internal(e.to_string()))?;
        Ok(accounts.iter().map(XeroEntityRef::from).collect())
    }

    pub fn fetch_invoices(&self, status: Option<&str>) -> Result<Value, ToolError> {
        let mut guard = self
            .client
            .lock()
            .map_err(|_| ToolError::Internal("lock poisoned".into()))?;
        let client = guard
            .as_mut()
            .ok_or_else(|| ToolError::Internal("Not authenticated with Xero".into()))?;
        let invoices = client
            .get_invoices(status)
            .map_err(|e| ToolError::Internal(e.to_string()))?;
        serde_json::to_value(&invoices).map_err(|e| ToolError::Internal(e.to_string()))
    }

    /// Upsert all Xero contacts + bank accounts + accounts into the ontology.
    pub fn sync_catalog(
        &self,
        store: &mut OntologyStore,
        ontology_path: &Path,
    ) -> Result<Value, ToolError> {
        let contacts = self.fetch_contacts(None)?;
        let bank_accounts = self.fetch_bank_accounts()?;
        let accounts = self.fetch_accounts()?;

        let mut entities: Vec<OntologyEntityInput> = Vec::new();

        for c in &contacts {
            let mut attrs = BTreeMap::new();
            attrs.insert("xero_id".into(), c.xero_id.clone());
            attrs.insert("display_name".into(), c.display_name.clone());
            attrs.insert("source".into(), "xero".into());
            entities.push(OntologyEntityInput {
                kind: OntologyEntityKind::XeroContact,
                attrs,
                custom_kind: None,
            });
        }

        for b in &bank_accounts {
            let mut attrs = BTreeMap::new();
            attrs.insert("xero_id".into(), b.xero_id.clone());
            attrs.insert("display_name".into(), b.display_name.clone());
            attrs.insert("source".into(), "xero".into());
            entities.push(OntologyEntityInput {
                kind: OntologyEntityKind::XeroBankAccount,
                attrs,
                custom_kind: None,
            });
        }

        for a in &accounts {
            let mut attrs = BTreeMap::new();
            attrs.insert("xero_id".into(), a.xero_id.clone());
            attrs.insert("display_name".into(), a.display_name.clone());
            attrs.insert("source".into(), "xero".into());
            entities.push(OntologyEntityInput {
                kind: OntologyEntityKind::Account,
                attrs,
                custom_kind: None,
            });
        }

        let inserted = store
            .upsert_entities(entities, None)
            .map_err(|e| ToolError::Internal(e.to_string()))?
            .inserted_count;

        store
            .persist(ontology_path)
            .map_err(|e| ToolError::Internal(e.to_string()))?;

        Ok(json!({
            "contacts_synced": contacts.len(),
            "bank_accounts_synced": bank_accounts.len(),
            "accounts_synced": accounts.len(),
            "entities_inserted": inserted,
        }))
    }
}

// ── Stub when feature is disabled ─────────────────────────────────────────────

#[cfg(not(feature = "xero"))]
pub struct XeroService;

#[cfg(not(feature = "xero"))]
impl XeroService {
    pub fn new_disabled() -> Self {
        Self
    }
}
