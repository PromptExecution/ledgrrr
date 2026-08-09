use std::path::Path;

use beankeeper_bridge::{convert, ConversionConfig};
use ledger_core::ingest::TransactionInput;

#[derive(Debug, thiserror::Error)]
pub enum ImportOfxError {
    #[error("OFX parse error: {0}")]
    OfxParse(String),
    #[error("bridge conversion error: {0}")]
    Bridge(#[from] beankeeper_bridge::BridgeError),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

pub fn parse_ofx_to_rows(
    ofx_path: &Path,
    config: &ConversionConfig,
) -> Result<Vec<TransactionInput>, ImportOfxError> {
    let content = std::fs::read_to_string(ofx_path)?;
    let doc = ofx_rs::parse(&content)
        .map_err(|e| ImportOfxError::OfxParse(e.to_string()))?;

    let mut rows: Vec<TransactionInput> = Vec::new();
    let source_ref = ofx_path.to_string_lossy().to_string();

    if let Some(banking) = doc.banking() {
        for wrapper in banking.statement_responses() {
            if let Some(stmt) = wrapper.response() {
                if let Some(tx_list) = stmt.transaction_list() {
                    for txn in tx_list.transactions() {
                        let _bridge = convert(txn, config)?;
                        let date_str = format_ofx_date(txn.date_posted());
                        let description = build_description(txn);
                        let amount_str = txn.amount().as_decimal().to_string();
                        rows.push(TransactionInput {
                            account_id: config.asset_name.clone(),
                            date: date_str,
                            amount: amount_str,
                            description,
                            source_ref: source_ref.clone(),
                        });
                    }
                }
            }
        }
    }

    if let Some(cc) = doc.credit_card() {
        for wrapper in cc.statement_responses() {
            if let Some(stmt) = wrapper.response() {
                if let Some(tx_list) = stmt.transaction_list() {
                    for txn in tx_list.transactions() {
                        let _bridge = convert(txn, config)?;
                        let date_str = format_ofx_date(txn.date_posted());
                        let description = build_description(txn);
                        let amount_str = txn.amount().as_decimal().to_string();
                        rows.push(TransactionInput {
                            account_id: config.asset_name.clone(),
                            date: date_str,
                            amount: amount_str,
                            description,
                            source_ref: source_ref.clone(),
                        });
                    }
                }
            }
        }
    }

    Ok(rows)
}

fn format_ofx_date(dt: &ofx_rs::types::OfxDateTime) -> String {
    let s = dt.to_string();
    if s.len() >= 8 {
        let ymd = &s[..8];
        format!("{}-{}-{}", &ymd[..4], &ymd[4..6], &ymd[6..8])
    } else {
        s
    }
}

fn build_description(txn: &ofx_rs::aggregates::StatementTransaction) -> String {
    let name = txn.name().unwrap_or("").trim();
    let memo = txn.memo().unwrap_or("").trim();
    if name.is_empty() && memo.is_empty() {
        "OFX import".to_string()
    } else if name.is_empty() {
        memo.to_string()
    } else if memo.is_empty() || memo == name {
        name.to_string()
    } else {
        format!("{} - {}", name, memo)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use beankeeper_bridge::OffsetKind;

    #[test]
    fn parse_ofx_to_rows_produces_transaction_inputs() {
        let ofx_content = r#"OFXHEADER:100
DATA:OFXSGML
VERSION:102
SECURITY:NONE
ENCODING:USASCII
CHARSET:1252
COMPRESSION:NONE
OLDFILEUID:NONE
NEWFILEUID:NONE

<OFX>
<SIGNONMSGSRSV1>
<SONRS>
<STATUS><CODE>0</CODE><SEVERITY>INFO</SEVERITY></STATUS>
<DTSERVER>20240115120000</DTSERVER>
<LANGUAGE>ENG</LANGUAGE>
</SONRS>
</SIGNONMSGSRSV1>
<BANKMSGSRSV1>
<STMTTRNRS>
<TRNUID>1</TRNUID>
<STATUS><CODE>0</CODE><SEVERITY>INFO</SEVERITY></STATUS>
<STMTRS>
<CURDEF>USD</CURDEF>
<BANKACCTFROM>
<BANKID>123</BANKID>
<ACCTID>456</ACCTID>
<ACCTTYPE>CHECKING</ACCTTYPE>
</BANKACCTFROM>
<BANKTRANLIST>
<DTSTART>20240101000000</DTSTART>
<DTEND>20240131000000</DTEND>
<STMTTRN>
<TRNTYPE>CREDIT</TRNTYPE>
<DTPOSTED>20240115</DTPOSTED>
<TRNAMT>1234.56</TRNAMT>
<FITID>FIT001</FITID>
<NAME>Salary</NAME>
</STMTTRN>
<STMTTRN>
<TRNTYPE>DEBIT</TRNTYPE>
<DTPOSTED>20240120</DTPOSTED>
<TRNAMT>-500.00</TRNAMT>
<FITID>FIT002</FITID>
<NAME>Rent</NAME>
<MEMO>Office rent</MEMO>
</STMTTRN>
</BANKTRANLIST>
<LEDGERBAL><BALAMT>734.56</BALAMT><DTASOF>20240131</DTASOF></LEDGERBAL>
</STMTRS>
</STMTTRNRS>
</BANKMSGSRSV1>
</OFX>"#;

        let tmp = std::env::temp_dir().join("test_import_ofx.qfx");
        std::fs::write(&tmp, ofx_content).unwrap();

        let config = ConversionConfig {
            asset_code: "1000".into(),
            asset_name: "Checking".into(),
            offset_code: "4000".into(),
            offset_name: "Revenue".into(),
            offset_kind: OffsetKind::Revenue,
        };

        let rows = parse_ofx_to_rows(&tmp, &config).expect("parse");
        assert_eq!(rows.len(), 2, "should extract 2 transactions");

        assert_eq!(rows[0].date, "2024-01-15");
        assert_eq!(rows[0].amount, "1234.56");
        assert_eq!(rows[0].description, "Salary");

        assert_eq!(rows[1].date, "2024-01-20");
        assert_eq!(rows[1].amount, "-500.00");
        assert_eq!(rows[1].description, "Rent - Office rent");

        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn import_ofx_error_on_missing_file() {
        let config = ConversionConfig {
            asset_code: "1000".into(),
            asset_name: "Checking".into(),
            offset_code: "4000".into(),
            offset_name: "Revenue".into(),
            offset_kind: OffsetKind::Revenue,
        };
        let result = parse_ofx_to_rows(
            Path::new("/nonexistent/ofx_file.qfx"),
            &config,
        );
        assert!(result.is_err());
    }
}
