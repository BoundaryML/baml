use std::{collections::BTreeMap, path::PathBuf};

use anyhow::{Context, Result};
use bex_events::value::{ValueFileRecord, read_bamlvalue_from_bytes};
use serde::Serialize;

use crate::row::ArtifactIdentity;

#[derive(Debug, Serialize)]
pub(crate) struct ValueStats {
    schema_version: u32,
    artifact: ArtifactIdentity,
    records: usize,
    record_counts: BTreeMap<&'static str, u64>,
    inline_body_bytes: u64,
    referenced_blob_bytes: u64,
    capture_loss_records: u64,
    capture_loss_items: u64,
    truncated: bool,
}

pub(crate) fn inspect_all(paths: &[PathBuf]) -> Result<Vec<ValueStats>> {
    paths
        .iter()
        .map(|path| {
            let (artifact, bytes) =
                ArtifactIdentity::read(path).with_context(|| format!("read {}", path.display()))?;
            let contents = read_bamlvalue_from_bytes(&bytes)
                .with_context(|| format!("decode {}", path.display()))?;
            let mut record_counts = BTreeMap::new();
            let mut inline_body_bytes = 0u64;
            let mut referenced_blob_bytes = 0u64;
            let mut capture_loss_records = 0u64;
            let mut capture_loss_items = 0u64;
            for record in &contents.records {
                let kind = match record {
                    ValueFileRecord::CapturedValue(value) => {
                        inline_body_bytes =
                            inline_body_bytes.saturating_add(value.body.len() as u64);
                        referenced_blob_bytes = referenced_blob_bytes.saturating_add(
                            value
                                .blob_ref
                                .as_ref()
                                .map_or(0, |blob| blob.size_bytes as u64),
                        );
                        "captured_value"
                    }
                    ValueFileRecord::LogEvent(value) => {
                        inline_body_bytes =
                            inline_body_bytes.saturating_add(value.body.len() as u64);
                        referenced_blob_bytes = referenced_blob_bytes.saturating_add(
                            value
                                .blob_ref
                                .as_ref()
                                .map_or(0, |blob| blob.size_bytes as u64),
                        );
                        "log_event"
                    }
                    ValueFileRecord::CaptureLoss(loss) => {
                        capture_loss_records = capture_loss_records.saturating_add(1);
                        capture_loss_items = capture_loss_items.saturating_add(loss.skipped_count);
                        "capture_loss"
                    }
                    ValueFileRecord::Audit(
                        bex_events::value::ValueAuditRecord::CapturePolicyChanged(_),
                    ) => "capture_policy_changed",
                    ValueFileRecord::Audit(
                        bex_events::value::ValueAuditRecord::PromotionOccurred(_),
                    ) => "promotion_occurred",
                    ValueFileRecord::RunStarted(_) => "run_started",
                    ValueFileRecord::RunCompleted(_) => "run_completed",
                };
                *record_counts.entry(kind).or_default() += 1;
            }
            Ok(ValueStats {
                schema_version: 1,
                artifact,
                records: contents.records.len(),
                record_counts,
                inline_body_bytes,
                referenced_blob_bytes,
                capture_loss_records,
                capture_loss_items,
                truncated: contents.truncated,
            })
        })
        .collect()
}
