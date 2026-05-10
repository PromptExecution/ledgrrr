//! Batch execution engine for review operations.
//!
//! This module provides a generic executor for processing batches of transactions
//! with configurable error handling modes (all-or-nothing vs. continue-on-error).

use std::time::Instant;

use crate::contract::{
    BatchItemResult, BatchItemStatus, BatchMode, BatchSummary,
};
use crate::ToolError;

/// Generic batch executor that applies an operation to a list of transaction IDs.
///
/// # Arguments
///
/// * `tx_ids` - List of transaction IDs to process
/// * `batch_mode` - Whether to stop on first error (AllOrNothing) or continue
/// * `dry_run` - If true, skip all operations and return skipped status
/// * `operation` - Closure that processes a single tx_id and returns a BatchItemResult
///
/// # Returns
///
/// A summary of batch execution with success/failure counts and total duration
pub struct BatchExecutor;

impl BatchExecutor {
    /// Execute a batch operation across multiple transaction IDs.
    ///
    /// The operation closure is called for each tx_id in sequence.
    /// - In `AllOrNothing` mode, processing stops on first error
    /// - In `ContinueOnError` mode, all tx_ids are attempted
    /// - In `dry_run` mode, all operations are skipped
    pub fn execute_batch(
        tx_ids: Vec<String>,
        batch_mode: BatchMode,
        dry_run: bool,
        mut operation: impl FnMut(&str) -> Result<BatchItemResult, ToolError>,
    ) -> Result<BatchSummary, ToolError> {
        let start = Instant::now();
        let total_requested = tx_ids.len();

        if total_requested == 0 {
            return Ok(BatchSummary {
                total_requested: 0,
                succeeded: 0,
                failed: 0,
                skipped: 0,
                batch_duration_ms: 0,
            });
        }

        let mut succeeded = 0;
        let mut failed = 0;
        let mut skipped = 0;

        for tx_id in &tx_ids {
            let _status = if dry_run {
                skipped += 1;
                BatchItemStatus::Skipped {
                    reason: "dry_run".to_string(),
                }
            } else {
                match operation(tx_id) {
                    Ok(result) => {
                        succeeded += 1;
                        result.status
                    }
                    Err(e) => {
                        failed += 1;
                        if batch_mode == BatchMode::AllOrNothing {
                            // Stop processing on first error
                            break;
                        }
                        BatchItemStatus::Failed { error: e.to_string() }
                    }
                }
            };

            // Note: items are populated by the caller with audit entries
        }

        let duration = start.elapsed().as_millis() as u64;

        Ok(BatchSummary {
            total_requested,
            succeeded,
            failed,
            skipped,
            batch_duration_ms: duration,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_execute_batch_empty_list() {
        let summary = BatchExecutor::execute_batch(
            vec![],
            BatchMode::ContinueOnError,
            false,
            |_tx_id| Ok(BatchItemResult {
                tx_id: "test".to_string(),
                status: BatchItemStatus::Succeeded,
                audit_entries: vec![],
            }),
        )
        .unwrap();

        assert_eq!(summary.total_requested, 0);
        assert_eq!(summary.succeeded, 0);
        assert_eq!(summary.failed, 0);
        assert_eq!(summary.skipped, 0);
    }

    #[test]
    fn test_execute_batch_all_succeed() {
        let summary = BatchExecutor::execute_batch(
            vec!["tx1".to_string(), "tx2".to_string(), "tx3".to_string()],
            BatchMode::ContinueOnError,
            false,
            |tx_id| Ok(BatchItemResult {
                tx_id: tx_id.to_string(),
                status: BatchItemStatus::Succeeded,
                audit_entries: vec![],
            }),
        )
        .unwrap();

        assert_eq!(summary.total_requested, 3);
        assert_eq!(summary.succeeded, 3);
        assert_eq!(summary.failed, 0);
        assert_eq!(summary.skipped, 0);
    }

    #[test]
    fn test_execute_batch_continue_on_error() {
        let mut call_count = 0;
        let summary = BatchExecutor::execute_batch(
            vec!["tx1".to_string(), "tx2".to_string(), "tx3".to_string()],
            BatchMode::ContinueOnError,
            false,
            |tx_id| {
                call_count += 1;
                if tx_id == "tx2" {
                    Err(ToolError::InvalidInput("test error".to_string()))
                } else {
                    Ok(BatchItemResult {
                        tx_id: tx_id.to_string(),
                        status: BatchItemStatus::Succeeded,
                        audit_entries: vec![],
                    })
                }
            },
        )
        .unwrap();

        assert_eq!(call_count, 3); // All three attempted
        assert_eq!(summary.total_requested, 3);
        assert_eq!(summary.succeeded, 2);
        assert_eq!(summary.failed, 1);
        assert_eq!(summary.skipped, 0);
    }

    #[test]
    fn test_execute_batch_all_or_nothing_stops_on_error() {
        let mut call_count = 0;
        let summary = BatchExecutor::execute_batch(
            vec!["tx1".to_string(), "tx2".to_string(), "tx3".to_string()],
            BatchMode::AllOrNothing,
            false,
            |tx_id| {
                call_count += 1;
                if tx_id == "tx2" {
                    Err(ToolError::InvalidInput("test error".to_string()))
                } else {
                    Ok(BatchItemResult {
                        tx_id: tx_id.to_string(),
                        status: BatchItemStatus::Succeeded,
                        audit_entries: vec![],
                    })
                }
            },
        )
        .unwrap();

        assert_eq!(call_count, 2); // tx1 succeeded, tx2 failed, tx3 never attempted
        assert_eq!(summary.total_requested, 3);
        assert_eq!(summary.succeeded, 1);
        assert_eq!(summary.failed, 1);
        assert_eq!(summary.skipped, 0);
    }

    #[test]
    fn test_execute_batch_dry_run() {
        let mut call_count = 0;
        let summary = BatchExecutor::execute_batch(
            vec!["tx1".to_string(), "tx2".to_string()],
            BatchMode::ContinueOnError,
            true,
            |_tx_id| {
                call_count += 1;
                Err(ToolError::Internal("should not be called".to_string()))
            },
        )
        .unwrap();

        assert_eq!(call_count, 0); // Operation never called in dry_run mode
        assert_eq!(summary.total_requested, 2);
        assert_eq!(summary.succeeded, 0);
        assert_eq!(summary.failed, 0);
        assert_eq!(summary.skipped, 2);
    }
}
