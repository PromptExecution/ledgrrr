# Rule Ontology

Mapping of each Rhai rule file to its tax-code anchor, jurisdiction, signal type, output categories, and review conditions.

Rules below are generic and jurisdiction-level by design. For client-specific
vendor/account classification rules (untracked, operator-maintained), see
[`CLIENT_RULES.md`](./CLIENT_RULES.md).

| Rule File | Tax Code Section | Jurisdiction | Signal Type | TaxCategory Output(s) | Review Condition |
|---|---|---|---|---|---|
| `classify_schedule_c.rhai` | IRC §162(a) | US | Description keywords: "invoice", "client payment", "consulting fee", "freelance payment", "1099", "contract payment", "business expense", "office supply", "software subscription", "professional development", "business meal" | `SelfEmployment`, `OfficeSupplies` | Income > $5,000 or expense abs(amount) > $2,500 |
| `classify_schedule_d.rhai` | IRC §1222(1), §1222(3) | US | Description keywords: "sale proceeds", "stock sale", "equity sale", "covered call", "option exercise", "brokerage sale", "ETF sale" | `CapitalGain`, `CapitalLoss` | Description contains "short" or "< 1 year" (STCG taxed as ordinary income) |
| `classify_schedule_e.rhai` | IRC §469(c)(1) | US | Description keywords: "rent", "rental income", "royalty", "K-1", "partnership distribution", "LLC distribution", "S-corp distribution", "sublease" | `RentalIncome` | Amount > $10,000 (passive loss rules); always review on negative amount |
| `classify_fbar.rhai` | 31 USC §5314; FinCEN Form 114 | US | account_id: "HSBC", "SWIFT", "IBAN", "BIC"; description: "wire transfer", "international transfer", "SEPA", "foreign bank" | `ForeignIncome` | abs(amount) > $9,000 (approaching $10,000 FBAR aggregate threshold) |
| `classify_fatca.rhai` | IRC §6038D; Form 8938 | US | account_id: "HSBC", "SWIFT"; description: "foreign financial", "offshore", "foreign investment" | `ForeignIncome` | abs(amount) > $25,000 (materially relevant to Form 8938 thresholds) |
| `classify_crypto_trading.rhai` | IRS Notice 2014-21; Rev. Proc. 2024-28 | US | Description keywords — BUY: "crypto purchase", "BTC buy", "ETH buy", "coin purchase", "exchange deposit"; SELL: "crypto sale", "BTC sell", "ETH sell", "coin sale", "exchange withdrawal", "crypto exchange" | `Transfer` (buy), `CryptoGain` (positive sell), `CapitalLoss` (negative sell) | Always review on sell/exchange events (must compute basis and holding period) |
| `classify_crypto_staking.rhai` | Rev. Rul. 2023-14; IRC §61(a)(1) | US | Description keywords: "staking reward", "validator reward", "proof of stake", "staking income", "airdrop", "DeFi yield", "liquidity mining", "yield farming" | `CryptoIncome` | Always review (must determine FMV at receipt for ordinary income computation) |
| `classify_au_gst.rhai` | A New Tax System (GST) Act 1999 s.11-5 | AU | account_id: "AU", "AUS", "ANZ", "CBA", "NAB", "WBC"; description: "GST", "tax invoice", "ABN", "BAS" | `AuGst` (expense), `ForeignIncome` (income) | Always review on positive AU income (US expat foreign income reporting obligation) |
| `classify_au_cgt.rhai` | ITAA 1997 s.115-A | AU | Description keywords: "property sale", "real estate settlement", "shares sale", "investment property", "capital proceeds"; AND account_id contains "AU"/"AUS" or description contains "AUD" | `AuCgt` | Always review (50% CGT discount eligibility requires acquisition date verification; also US reporting required) |

## Notes

- All rules follow the `fn classify(tx)` API contract: accept a map `{ tx_id, account_id, date, amount, description }`, return `#{ category, confidence, review, reason }`.
- `amount` is always a string decimal. Rules use `parse_float()` for numeric comparisons only — no floating-point money math.
- Rules return `Unclassified` with `confidence: 0.0` when no signal matches, allowing the engine to continue to the next rule in a chain.
- Review conditions set `review: true` in the rule return; the Rust `ClassificationEngine` also applies its own `review_threshold` on top.
- FBAR and FATCA rules detect individual transaction signals only — aggregate balance across all foreign accounts must be computed by the Rust pipeline.
