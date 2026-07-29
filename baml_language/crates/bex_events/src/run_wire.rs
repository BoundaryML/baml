use serde_json::{Value, json};

use crate::{
    run::{
        CallNode, CallStatus, CapturedValueRole, DiagnosticSeverity, EnvResolutionStatus,
        PayloadBody, PayloadBodyState, PayloadEvent, PayloadId, PayloadKind, Run, RunDiagnostic,
        RunError, RunOutcome, RunPatch, RunPatchChange, RunRequestState, RunResult, RunStatus,
        RunSummary, RunTarget, RunVisibility, ThreadNode, ThreadStatus,
    },
    value::ValueRef,
};

pub fn run_to_wire(run: &Run) -> Value {
    json!({
        "boundaryId": run.boundary_id.to_wire_string(),
        "target": target_to_wire(&run.target),
        "visibility": visibility_to_wire(&run.visibility),
        "status": status_to_wire(run.status),
        "createdAtMs": run.created_at_ms,
        "startedAtMs": run.started_at_ms,
        "completedAtMs": run.completed_at_ms,
        "timeAnchor": {
            "epochCreatedAtMs": run.time_anchor.epoch_created_at_ms,
            "traceZeroNs": run.time_anchor.trace_zero_ns.to_string(),
        },
        "request": {
            "projectId": run.request.project_id.0,
            "projectGeneration": run.request.project_generation.0,
            "target": target_to_wire(&run.request.target),
            "argsSummary": run.request.args_summary,
            "optionsSummary": run.request.options_summary,
        },
        "result": run.result.as_ref().map(result_to_wire),
        "error": run.error.as_ref().map(error_to_wire),
        "cancellation": run.cancellation.as_ref().map(|cancellation| json!({
            "requestedAtMs": cancellation.requested_at_ms,
            "completedAtMs": cancellation.completed_at_ms,
            "reason": cancellation.reason,
        })),
        "rootCallNodeId": run.root_call_node_id.map(call_node_id_to_wire),
        "graphRuntimeOverlay": run.graph_runtime_overlay.as_ref().map(graph_runtime_overlay_to_wire),
        "calls": run.calls.iter().map(call_to_wire).collect::<Vec<_>>(),
        "threads": run.threads.iter().map(thread_to_wire).collect::<Vec<_>>(),
        "payloads": run.payloads.iter().map(payload_to_wire).collect::<Vec<_>>(),
        "diagnostics": run.diagnostics.iter().map(diagnostic_to_wire).collect::<Vec<_>>(),
        "cursor": run.cursor.0,
    })
}

pub fn patch_to_wire(patch: &RunPatch) -> Value {
    json!({
        "boundaryId": patch.boundary_id.to_wire_string(),
        "cursor": patch.cursor.0,
        "changes": patch.changes.iter().map(patch_change_to_wire).collect::<Vec<_>>(),
    })
}

pub fn run_summary_to_wire(summary: &RunSummary) -> Value {
    json!({
        "boundaryId": summary.boundary_id.to_wire_string(),
        "target": target_to_wire(&summary.target),
        "visibility": visibility_to_wire(&summary.visibility),
        "status": status_to_wire(summary.status),
        "request": {
            "projectId": &summary.request.project_id.0,
            "projectGeneration": summary.request.project_generation.0,
            "target": target_to_wire(&summary.request.target),
            "argsSummary": summary.request.args_summary.as_ref(),
            "optionsSummary": summary.request.options_summary.as_ref(),
        },
        "touchedFunctions": &summary.touched_functions,
        "createdAtMs": summary.created_at_ms,
        "completedAtMs": summary.completed_at_ms,
        "retention": format!("{:?}", summary.retention),
    })
}

fn target_to_wire(target: &RunTarget) -> Value {
    match target {
        RunTarget::Function { function_name } => {
            json!({ "kind": "function", "functionName": function_name })
        }
        RunTarget::Test {
            generation,
            test_name,
        } => json!({ "kind": "test", "generation": generation.0, "testName": test_name }),
        RunTarget::Preview {
            parent_function_name,
            helper,
        } => {
            json!({ "kind": "preview", "parentFunctionName": parent_function_name, "helper": helper })
        }
        RunTarget::Companion {
            parent_boundary_id,
            function_name,
        } => json!({
            "kind": "companion",
            "parentBoundaryId": parent_boundary_id.map(super::run::BoundaryId::to_wire_string),
            "functionName": function_name,
        }),
        RunTarget::Internal { name } => json!({ "kind": "internal", "name": name }),
    }
}

fn visibility_to_wire(visibility: &RunVisibility) -> Value {
    match visibility {
        RunVisibility::History => json!({ "kind": "history" }),
        RunVisibility::Scoped { scope_id } => json!({ "kind": "scoped", "scopeId": scope_id }),
        RunVisibility::Hidden => json!({ "kind": "hidden" }),
        RunVisibility::DebugOnly => json!({ "kind": "debugOnly" }),
    }
}

fn patch_change_to_wire(change: &RunPatchChange) -> Value {
    match change {
        RunPatchChange::UpsertCallNode(call) => {
            json!({ "type": "upsertCallNode", "call": call_to_wire(call) })
        }
        RunPatchChange::UpsertThreadNode(thread) => {
            json!({ "type": "upsertThreadNode", "thread": thread_to_wire(thread) })
        }
        RunPatchChange::UpsertPayload(payload) => {
            json!({ "type": "upsertPayload", "payload": payload_to_wire(payload) })
        }
        RunPatchChange::UpsertDiagnostic(diagnostic) => {
            json!({ "type": "upsertDiagnostic", "diagnostic": diagnostic_to_wire(diagnostic) })
        }
        RunPatchChange::SetRootCallNode(id) => {
            json!({ "type": "setRootCallNode", "callNodeId": id.map(call_node_id_to_wire) })
        }
        RunPatchChange::SetGraphRuntimeOverlay(overlay) => {
            json!({ "type": "setGraphRuntimeOverlay", "overlay": graph_runtime_overlay_to_wire(overlay) })
        }
        RunPatchChange::SetStatus(status) => {
            json!({ "type": "setStatus", "status": status_to_wire(*status) })
        }
        RunPatchChange::Complete(outcome) => {
            json!({ "type": "complete", "outcome": outcome_to_wire(outcome) })
        }
    }
}

fn thread_to_wire(thread: &ThreadNode) -> Value {
    json!({
        "id": thread_node_id_to_wire(thread.id),
        "parentThreadId": thread.parent_thread_id.map(thread_node_id_to_wire),
        "parentCallNodeId": thread.parent_call_node_id.map(call_node_id_to_wire),
        "name": thread.name,
        "startedAtNs": thread.started_at_ns.map(|value| value.to_string()),
        "endedAtNs": thread.ended_at_ns.map(|value| value.to_string()),
        "status": thread_status_to_wire(thread.status),
        "callNodeIds": thread.call_node_ids.iter().copied().map(call_node_id_to_wire).collect::<Vec<_>>(),
    })
}

fn call_to_wire(call: &CallNode) -> Value {
    json!({
        "id": call_node_id_to_wire(call.id),
        "threadId": thread_node_id_to_wire(call.thread_id),
        "parentId": call.parent_id.map(call_node_id_to_wire),
        "functionId": call.function_id.0,
        "functionName": call.function_name,
        "functionOrigin": call.function_origin.as_ref().map(function_origin_to_wire),
        "calleeSource": call.callee_source.as_ref().map(source_location_to_wire),
        "callSiteSource": call.call_site_source.as_ref().map(source_location_to_wire),
        "startedAtNs": call.started_at_ns.map(|value| value.to_string()),
        "endedAtNs": call.ended_at_ns.map(|value| value.to_string()),
        "status": call_status_to_wire(call.status),
        "payloadIds": call.payload_ids.iter().copied().map(payload_id_to_wire).collect::<Vec<_>>(),
    })
}

fn graph_runtime_overlay_to_wire(overlay: &crate::run::GraphRuntimeOverlay) -> Value {
    json!({
        "boundaryId": overlay.boundary_id.to_wire_string(),
        "projectGeneration": overlay.project_generation.0,
        "entries": overlay.entries.iter().map(|entry| json!({
            "cfgNodeId": entry.cfg_node_id.0,
            "callNodeIds": entry.call_node_ids.iter().copied().map(call_node_id_to_wire).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
        "unattachedCallNodeIds": overlay.unattached_call_node_ids.iter().copied().map(call_node_id_to_wire).collect::<Vec<_>>(),
        "diagnostics": overlay.diagnostics.iter().map(diagnostic_to_wire).collect::<Vec<_>>(),
    })
}

fn function_origin_to_wire(origin: &crate::run::FunctionOrigin) -> &'static str {
    match origin {
        crate::run::FunctionOrigin::User => "user",
        crate::run::FunctionOrigin::Builtin => "builtin",
        crate::run::FunctionOrigin::Companion => "companion",
        crate::run::FunctionOrigin::Internal => "internal",
        crate::run::FunctionOrigin::Unknown => "unknown",
    }
}

fn source_location_to_wire(source: &crate::run::SourceLocation) -> Value {
    json!({
        "filePath": source.file_path.as_ref(),
        "fileId": source.file_id,
        "line": source.line,
        "column": source.column,
        "endLine": source.end_line,
        "endColumn": source.end_column,
        "startOffset": source.start_offset,
        "endOffset": source.end_offset,
    })
}

fn payload_to_wire(payload: &PayloadEvent) -> Value {
    json!({
        "id": payload.payload_id_wire(),
        "callNodeId": payload.call_node_id.map(call_node_id_to_wire),
        "timestampMs": payload.timestamp_ms,
        "kind": payload_kind_to_wire(&payload.kind),
        "redaction": {
            "valueRedacted": payload.redaction.value_redacted,
            "displaySafe": payload.redaction.display_safe,
            "reason": payload.redaction.reason.as_ref(),
            "policyId": payload.redaction.policy_id.as_ref(),
        },
        "body": payload.body.as_ref().map(payload_body_to_wire),
    })
}

trait PayloadWireExt {
    fn payload_id_wire(&self) -> String;
}

impl PayloadWireExt for PayloadEvent {
    fn payload_id_wire(&self) -> String {
        payload_id_to_wire(self.id)
    }
}

fn payload_id_to_wire(id: PayloadId) -> String {
    format!("payload_{}", id.0)
}

fn payload_kind_to_wire(kind: &PayloadKind) -> Value {
    match kind {
        PayloadKind::FetchStarted(fetch) => json!({
            "type": "fetchStarted",
            "fetchId": fetch.fetch_id.to_string(),
            "method": &fetch.method,
            "url": &fetch.url,
            "requestHeaders": fetch.request_headers.iter().map(|header| json!({
                "name": &header.name,
                "valueRedacted": header.value_redacted,
                "value": header.value.as_ref(),
            })).collect::<Vec<_>>(),
        }),
        PayloadKind::FetchUpdated(fetch) => json!({
            "type": "fetchUpdated",
            "fetchId": fetch.fetch_id.to_string(),
            "status": fetch.status,
            "durationMs": fetch.duration_ms,
            "responseHeaders": fetch.response_headers.iter().map(|header| json!({
                "name": &header.name,
                "valueRedacted": header.value_redacted,
                "value": header.value.as_ref(),
            })).collect::<Vec<_>>(),
            "error": fetch.error.as_ref(),
        }),
        PayloadKind::InputRequested(input) => json!({
            "type": "inputRequested",
            "requestId": input.request_id.to_string(),
            "prompt": input.prompt.as_ref(),
            "state": request_state_to_wire(input.state),
        }),
        PayloadKind::InputResolved(input) => json!({
            "type": "inputResolved",
            "requestId": input.request_id.to_string(),
            "state": request_state_to_wire(input.state),
        }),
        PayloadKind::EnvRequested(env) => json!({
            "type": "envRequested",
            "requestId": env.request_id.to_string(),
            "key": &env.key,
            "state": request_state_to_wire(env.state),
            "waiterCount": env.waiter_count,
        }),
        PayloadKind::EnvResolved(env) => json!({
            "type": "envResolved",
            "requestId": env.request_id.to_string(),
            "key": &env.key,
            "status": env_resolution_status_to_wire(env.status),
            "state": request_state_to_wire(env.state),
            "valueRedacted": env.value_redacted,
            "displayValue": env.display_value.as_ref(),
        }),
        PayloadKind::Log(log) => json!({
            "type": "log",
            "level": log.level.as_ref(),
            "message": &log.message,
            "source": log.source.as_ref().map(source_location_to_wire),
            "valueRef": log.value_ref.as_ref().map(value_ref_to_wire),
        }),
        PayloadKind::Output(output) => json!({
            "type": "output",
            "stream": output.stream.as_wire_str(),
            "text": &output.text,
        }),
        PayloadKind::CapturedValue(captured) => json!({
            "type": "capturedValue",
            "role": captured_value_role_to_wire(captured.role),
            "label": captured.label.as_ref(),
            "valueRef": captured.value_ref.as_ref().map(value_ref_to_wire),
        }),
    }
}

fn captured_value_role_to_wire(role: CapturedValueRole) -> &'static str {
    match role {
        CapturedValueRole::RootInput => "rootInput",
        CapturedValueRole::CallInput => "callInput",
        CapturedValueRole::CallOutput => "callOutput",
        CapturedValueRole::CallError => "callError",
    }
}

fn value_ref_to_wire(value_ref: &ValueRef) -> Value {
    json!({
        "id": value_ref.id,
        "codec": value_ref.codec.as_wire_str(),
        "availability": value_ref.availability.as_wire_str(),
        "originalSizeBytes": value_ref.original_size_bytes,
        "retainedSizeBytes": value_ref.retained_size_bytes,
        "diagnostic": value_ref.diagnostic.as_ref(),
    })
}

fn result_to_wire(result: &RunResult) -> Value {
    json!({
        "valueRef": result.value_ref.as_ref().map(value_ref_to_wire),
        "rendererHint": result.renderer_hint,
        "supportingPayloadIds": result.supporting_payload_ids.iter().copied().map(payload_id_to_wire).collect::<Vec<_>>(),
    })
}

fn error_to_wire(error: &RunError) -> Value {
    json!({
        "class": format!("{:?}", error.class),
        "message": error.message,
        "details": error.details,
        "valueRef": error.value_ref.as_ref().map(value_ref_to_wire),
    })
}

fn payload_body_to_wire(body: &PayloadBody) -> Value {
    json!({
        "state": payload_body_state_to_wire(&body.state),
        "contentType": body.content_type.as_ref(),
        "originalSizeBytes": body.original_size_bytes,
        "retainedSizeBytes": body.retained_size_bytes,
    })
}

fn payload_body_state_to_wire(state: &PayloadBodyState) -> Value {
    match state {
        PayloadBodyState::InlineBytes => json!({ "kind": "inlineBytes" }),
        PayloadBodyState::InlineJson => json!({ "kind": "inlineJson" }),
        PayloadBodyState::RetainedByRef(reference) => {
            json!({ "kind": "retainedByRef", "id": &reference.id })
        }
        PayloadBodyState::Truncated => json!({ "kind": "truncated" }),
        PayloadBodyState::Compacted => json!({ "kind": "compacted" }),
        PayloadBodyState::OmittedByPolicy => json!({ "kind": "omittedByPolicy" }),
    }
}

fn diagnostic_to_wire(diagnostic: &RunDiagnostic) -> Value {
    json!({
        "severity": severity_to_wire(diagnostic.severity),
        "code": diagnostic.code,
        "message": diagnostic.message,
        "callNodeId": diagnostic.call_node_id.map(call_node_id_to_wire),
        "payloadId": diagnostic.payload_id.map(payload_id_to_wire),
    })
}

fn outcome_to_wire(outcome: &RunOutcome) -> Value {
    match outcome {
        RunOutcome::Succeeded(result) => json!({
            "status": "succeeded",
            "result": result_to_wire(result),
        }),
        RunOutcome::Failed(error) => json!({
            "status": "failed",
            "error": error_to_wire(error),
        }),
        RunOutcome::Cancelled(cancellation) => json!({
            "status": "cancelled",
            "cancellation": {
                "requestedAtMs": cancellation.requested_at_ms,
                "completedAtMs": cancellation.completed_at_ms,
                "reason": cancellation.reason,
            },
        }),
        RunOutcome::Panicked(error) => json!({
            "status": "panicked",
            "error": error_to_wire(error),
        }),
    }
}

fn status_to_wire(status: RunStatus) -> &'static str {
    match status {
        RunStatus::Pending => "pending",
        RunStatus::Running => "running",
        RunStatus::WaitingForInput => "waitingForInput",
        RunStatus::WaitingForEnv => "waitingForEnv",
        RunStatus::Cancelling => "cancelling",
        RunStatus::Succeeded => "succeeded",
        RunStatus::Failed => "failed",
        RunStatus::Cancelled => "cancelled",
        RunStatus::Panicked => "panicked",
    }
}

fn thread_status_to_wire(status: ThreadStatus) -> &'static str {
    match status {
        ThreadStatus::Running => "running",
        ThreadStatus::Completed => "completed",
        ThreadStatus::Cancelled => "cancelled",
        ThreadStatus::Errored => "errored",
    }
}

fn call_status_to_wire(status: CallStatus) -> &'static str {
    match status {
        CallStatus::Running => "running",
        CallStatus::Ok => "ok",
        CallStatus::Errored => "errored",
        CallStatus::Cancelled => "cancelled",
        CallStatus::Exited => "exited",
    }
}

fn request_state_to_wire(state: RunRequestState) -> &'static str {
    match state {
        RunRequestState::Pending => "pending",
        RunRequestState::Resolved => "resolved",
        RunRequestState::Cancelled => "cancelled",
        RunRequestState::Expired => "expired",
        RunRequestState::RunTerminal => "runTerminal",
    }
}

fn env_resolution_status_to_wire(status: EnvResolutionStatus) -> &'static str {
    match status {
        EnvResolutionStatus::ResolvedFromOverride => "resolvedFromOverride",
        EnvResolutionStatus::ResolvedFromProcess => "resolvedFromProcess",
        EnvResolutionStatus::ResolvedFromUser => "resolvedFromUser",
        EnvResolutionStatus::DeclinedMissing => "declinedMissing",
    }
}

fn severity_to_wire(severity: DiagnosticSeverity) -> &'static str {
    match severity {
        DiagnosticSeverity::Error => "error",
        DiagnosticSeverity::Warning => "warning",
        DiagnosticSeverity::Info => "info",
    }
}

fn call_node_id_to_wire(id: crate::run::CallNodeId) -> String {
    format!("call_node_{}", id.get())
}

fn thread_node_id_to_wire(id: crate::run::ThreadNodeId) -> String {
    format!("thread_node_{}", id.get())
}
