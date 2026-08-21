use bex_events::run::{RunPatch, RunTarget};
use tokio::sync::broadcast;

use crate::playground_ws::WsOutMessage;

/// The function whose control-flow graph backs a run's graph overlay, if the
/// target kind has one.
pub fn overlay_function_name_for_target(target: &RunTarget) -> Option<&str> {
    match target {
        RunTarget::Function { function_name } | RunTarget::Companion { function_name, .. } => {
            Some(function_name)
        }
        RunTarget::Preview {
            parent_function_name,
            ..
        } => Some(parent_function_name),
        RunTarget::Test { .. } | RunTarget::Internal { .. } => None,
    }
}

pub fn broadcast_run_patch(broadcast_tx: &broadcast::Sender<WsOutMessage>, patch: &RunPatch) {
    let _ = broadcast_tx.send(WsOutMessage::RunPatch {
        patch: patch_to_wire(patch),
    });
}

pub use bex_events::run::{patch_to_wire, run_summary_to_wire, run_to_wire};

#[cfg(test)]
mod tests {
    use bex_events::run::{
        BoundaryId, ExecutionRequest, HeaderObservation, HostCallId, InMemoryRunStore,
        ProjectGeneration, ProjectId, RequestId, RunPatchChange, RunStatus, RunTarget,
    };

    use super::*;

    #[test]
    fn run_wire_projection_uses_opaque_ids_and_no_trace_refs() {
        let store = InMemoryRunStore::default();
        let start = store.create_run(
            BoundaryId::new_random(),
            ExecutionRequest {
                project_id: ProjectId("project".to_string()),
                project_generation: ProjectGeneration(1),
                target: RunTarget::Function {
                    function_name: "main".to_string(),
                },
                args_summary: None,
                options_summary: None,
            },
            RequestId(7),
        );
        let run = store.snapshot(start.boundary_id).unwrap();

        let wire = run_to_wire(&run);
        assert_eq!(wire["boundaryId"], start.boundary_id.to_wire_string());
        assert_eq!(wire["target"]["kind"], "function");
        let encoded = serde_json::to_string(&wire).unwrap();
        assert!(!encoded.contains("process_euid"));
        assert!(!encoded.contains("engine_id"));
        assert!(!encoded.contains("trace_key"));
        assert!(!encoded.contains("traceRef"));
    }

    #[test]
    fn patch_wire_projection_keeps_run_cursor_and_change_shape() {
        let store = InMemoryRunStore::default();
        let start = store.create_run(
            BoundaryId::new_random(),
            ExecutionRequest {
                project_id: ProjectId("project".to_string()),
                project_generation: ProjectGeneration(1),
                target: RunTarget::Function {
                    function_name: "main".to_string(),
                },
                args_summary: None,
                options_summary: None,
            },
            RequestId(7),
        );
        let patch = store
            .attach_host_call(
                start.boundary_id,
                bex_events::run::HostCallId::Native(sys_types::CallId(1)),
            )
            .unwrap();
        assert_eq!(
            patch.changes,
            vec![RunPatchChange::SetStatus(RunStatus::Running)]
        );

        let wire = patch_to_wire(&patch);
        assert_eq!(wire["boundaryId"], start.boundary_id.to_wire_string());
        assert_eq!(wire["cursor"], 1);
        assert_eq!(wire["changes"][0]["type"], "setStatus");
        assert_eq!(wire["changes"][0]["status"], "running");
    }

    #[test]
    fn payload_wire_projection_redacts_fetch_values() {
        let store = InMemoryRunStore::default();
        let start = store.create_run(
            BoundaryId::new_random(),
            ExecutionRequest {
                project_id: ProjectId("project".to_string()),
                project_generation: ProjectGeneration(1),
                target: RunTarget::Function {
                    function_name: "main".to_string(),
                },
                args_summary: None,
                options_summary: None,
            },
            RequestId(7),
        );
        let host_call_id = HostCallId::Native(sys_types::CallId(9));
        store.attach_host_call(start.boundary_id, host_call_id.clone());
        let patch = store
            .ingest_fetch_started(
                &host_call_id,
                5,
                "GET".to_string(),
                "https://example.test".to_string(),
                vec![HeaderObservation {
                    name: "authorization".to_string(),
                    value_redacted: true,
                    value: None,
                }],
                Some(24),
            )
            .unwrap();

        let run_wire = run_to_wire(&store.snapshot(start.boundary_id).unwrap());
        assert_eq!(run_wire["payloads"][0]["kind"]["type"], "fetchStarted");
        assert_eq!(
            run_wire["payloads"][0]["kind"]["requestHeaders"][0]["name"],
            "authorization"
        );
        assert_eq!(
            run_wire["payloads"][0]["kind"]["requestHeaders"][0]["valueRedacted"],
            true
        );
        assert_eq!(run_wire["payloads"][0]["redaction"]["valueRedacted"], true);
        assert_eq!(
            run_wire["payloads"][0]["body"]["state"]["kind"],
            "omittedByPolicy"
        );

        let patch_wire = patch_to_wire(&patch);
        assert_eq!(patch_wire["changes"][0]["type"], "upsertPayload");
        assert_eq!(
            patch_wire["changes"][0]["payload"]["kind"]["type"],
            "fetchStarted"
        );
        let encoded = serde_json::to_string(&run_wire).unwrap();
        assert!(!encoded.contains("Bearer secret"));
        assert!(!encoded.contains("process_euid"));
    }
}
