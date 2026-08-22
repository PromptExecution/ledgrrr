use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;

use crate::ingest::{deterministic_tx_id, TransactionInput};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JournalTransaction {
    pub date: String,
    pub payee: String,
    pub narration: String,
    pub asset_account: String,
    pub counterparty_account: String,
    pub amount: String,
    pub currency: String,
    pub tx_id: String,
    pub source_ref: String,
}

impl JournalTransaction {
    pub fn from_input(row: &TransactionInput) -> Self {
        let tx_id = deterministic_tx_id(row);
        Self {
            date: row.date.clone(),
            payee: "Imported".to_string(),
            narration: row.description.clone(),
            asset_account: format!("Assets:Bank:{}", row.account_id),
            counterparty_account: "Equity:Suspense:Imported".to_string(),
            amount: row.amount.clone(),
            currency: "USD".to_string(),
            tx_id,
            source_ref: row.source_ref.clone(),
        }
    }

    pub fn to_beancount_entry(&self) -> String {
        let inverse = invert_amount(&self.amount);
        format!(
            "{} * \"{}\" \"{}\"\n  txid: \"{}\"\n  source_ref: \"{}\"\n  {} {} {}\n  {} {} {}\n",
            self.date,
            self.payee,
            self.narration.replace('"', "'"),
            self.tx_id,
            self.source_ref.replace('"', "'"),
            self.asset_account,
            self.amount,
            self.currency,
            self.counterparty_account,
            inverse,
            self.currency
        )
    }
}

impl JournalTransaction {
    /// Constructor for agent-scoped token issuance (see
    /// elasticdotventures/_b00t_#1104 and
    /// docs/superpowers/specs/2026-08-22-agent-scoped-token-issuance.md).
    ///
    /// Unlike `from_input`, this does NOT depend on `TransactionInput` /
    /// `deterministic_tx_id` — agent-token issuance is not a tax-document
    /// import, it's a direct double-entry record of a k8s token mint event.
    /// `tx_id` is caller-supplied (the caller generates a fresh id per
    /// issuance, since each request is a distinct event, not an idempotent
    /// re-import of the same source data).
    ///
    /// Produces a balanced entry: `Assets:Cake:<agent_id>` debited by
    /// `cost`, `Expenses:AgentTokens:<shard-type>` credited by `cost`,
    /// where `<shard-type>` is the portion of `shard_ref` before the first
    /// `:` (e.g. `datum` from `datum:some-datum-id`).
    pub fn from_agent_token_issuance(
        agent_id: &str,
        shard_ref: &str,
        cost: &str,
        tx_id: String,
        date: String,
    ) -> Self {
        Self {
            date,
            payee: "AgentTokenIssuance".to_string(),
            narration: format!("agent token issued: {}", shard_ref),
            asset_account: format!("Assets:Cake:{}", agent_id),
            counterparty_account: format!(
                "Expenses:AgentTokens:{}",
                shard_ref.split(':').next().unwrap_or(shard_ref)
            ),
            amount: cost.to_string(),
            currency: "CAKE".to_string(),
            tx_id,
            source_ref: format!("agent-token:{}", agent_id),
        }
    }
}

pub fn append_entries(path: &Path, entries: &[JournalTransaction]) -> std::io::Result<()> {
    if entries.is_empty() {
        return Ok(());
    }

    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    for entry in entries {
        file.write_all(entry.to_beancount_entry().as_bytes())?;
        file.write_all(b"\n")?;
    }
    Ok(())
}

fn invert_amount(amount: &str) -> String {
    let trimmed = amount.trim();
    if let Some(rest) = trimmed.strip_prefix('-') {
        rest.to_string()
    } else if let Some(rest) = trimmed.strip_prefix('+') {
        format!("-{}", rest)
    } else {
        format!("-{}", trimmed)
    }
}

#[cfg(test)]
mod agent_token_issuance_tests {
    use super::*;

    #[test]
    fn from_agent_token_issuance_sets_expected_fields() {
        let tx = JournalTransaction::from_agent_token_issuance(
            "claude-worker-7",
            "datum:some-datum-id",
            "3",
            "tx-abc123".to_string(),
            "2026-08-22".to_string(),
        );

        assert_eq!(tx.date, "2026-08-22");
        assert_eq!(tx.payee, "AgentTokenIssuance");
        assert_eq!(tx.narration, "agent token issued: datum:some-datum-id");
        assert_eq!(tx.asset_account, "Assets:Cake:claude-worker-7");
        assert_eq!(tx.counterparty_account, "Expenses:AgentTokens:datum");
        assert_eq!(tx.amount, "3");
        assert_eq!(tx.currency, "CAKE");
        assert_eq!(tx.tx_id, "tx-abc123");
        assert_eq!(tx.source_ref, "agent-token:claude-worker-7");
    }

    #[test]
    fn from_agent_token_issuance_shard_ref_without_colon_falls_back_whole() {
        // Defensive: shard_ref should always contain a ':' in practice, but
        // the split().next().unwrap_or(shard_ref) fallback must not panic.
        let tx = JournalTransaction::from_agent_token_issuance(
            "agent-x",
            "datumonly",
            "1",
            "tx-1".to_string(),
            "2026-08-22".to_string(),
        );
        assert_eq!(tx.counterparty_account, "Expenses:AgentTokens:datumonly");
    }

    #[test]
    fn from_agent_token_issuance_produces_balanced_beancount_entry() {
        let tx = JournalTransaction::from_agent_token_issuance(
            "claude-worker-7",
            "datum:some-datum-id",
            "3",
            "tx-abc123".to_string(),
            "2026-08-22".to_string(),
        );

        let entry = tx.to_beancount_entry();

        // Debit leg: asset_account with the raw amount.
        assert!(entry.contains("Assets:Cake:claude-worker-7 3 CAKE"));
        // Credit leg: counterparty_account with the exact inverse amount.
        assert!(entry.contains("Expenses:AgentTokens:datum -3 CAKE"));

        // The two legs must be exact inverses of each other (balanced
        // double-entry), using the same `invert_amount` helper the rest of
        // the module relies on.
        assert_eq!(invert_amount(&tx.amount), "-3");
        assert_eq!(invert_amount(&invert_amount(&tx.amount)), tx.amount);

        // Sanity: header line present with payee/narration/txid/source_ref.
        assert!(entry.contains("2026-08-22 * \"AgentTokenIssuance\""));
        assert!(entry.contains("txid: \"tx-abc123\""));
        assert!(entry.contains("source_ref: \"agent-token:claude-worker-7\""));
    }

    #[test]
    fn from_agent_token_issuance_does_not_affect_from_input() {
        // Regression guard: from_input's behavior/contract is untouched by
        // this new constructor.
        let input = crate::ingest::TransactionInput {
            account_id: "acct-1".to_string(),
            date: "2026-08-22".to_string(),
            amount: "10.00".to_string(),
            description: "coffee".to_string(),
            source_ref: "src-1".to_string(),
        };
        let tx = JournalTransaction::from_input(&input);
        assert_eq!(tx.payee, "Imported");
        assert_eq!(tx.counterparty_account, "Equity:Suspense:Imported");
    }
}
