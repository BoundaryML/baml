//! Transport-independent run presentation shared by native and browser hosts.
use super::{
    BoundaryId, CancellationState, DiagnosticSeverity, InMemoryRunStore, RunDiagnostic, RunError,
    RunErrorClass, RunOutcome, RunPatch, RunResult,
};
use crate::value::ValueRef;

pub fn root_value_success_outcome_with_value(
    value_ref: Option<ValueRef>,
    value: Option<Vec<u8>>,
    renderer_hint: &str,
) -> RunOutcome {
    RunOutcome::Succeeded(RunResult {
        value_ref,
        value,
        renderer_hint: Some(renderer_hint.to_string()),
        supporting_payload_ids: Vec::new(),
    })
}

pub fn log_loss_diagnostic_patch(
    run_store: &InMemoryRunStore,
    boundary_id: BoundaryId,
    capture_kind: &str,
    skipped: u64,
) -> Option<RunPatch> {
    run_store.add_diagnostic(
        boundary_id,
        RunDiagnostic {
            severity: DiagnosticSeverity::Warning,
            code: Some("logCaptureLoss".to_string()),
            message: capture_loss_message(capture_kind, skipped),
            payload_id: None,
        },
    )
}

pub fn capture_loss_message(capture_kind: &str, skipped: u64) -> String {
    format!(
        "Skipped {skipped} captured {capture_kind} value(s) because the log capture queue was full"
    )
}

/// Render a host-classified failure. Hosts retain their cancellation detection
/// and clock; an explicit timestamp keeps the shared constructor deterministic.
pub fn error_outcome(
    message: String,
    class: RunErrorClass,
    value_ref: Option<ValueRef>,
    cancelled_at_ms: Option<u64>,
) -> RunOutcome {
    match cancelled_at_ms {
        Some(now) => RunOutcome::Cancelled(CancellationState {
            requested_at_ms: now,
            completed_at_ms: Some(now),
            reason: Some(message),
        }),
        None => RunOutcome::Failed(RunError {
            class,
            message,
            details: None,
            value_ref,
        }),
    }
}
