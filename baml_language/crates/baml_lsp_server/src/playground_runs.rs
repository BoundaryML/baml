use std::sync::Arc;

use bex_events::run::{
    CfgNodeId, CfgNodeSourceSpan, DiagnosticSeverity, GraphRuntimeOverlaySpanProvider,
    GraphRuntimeOverlaySpanResolution, InMemoryRunStore, ProfileEventEnvelope,
    ProfileEventObserver, Run, RunDiagnostic, RunPatch, RunTarget,
};
use tokio::sync::broadcast;

use crate::playground_ws::WsOutMessage;

pub struct ProjectGraphRuntimeOverlaySpanProvider {
    bex: Arc<dyn bex_project::BexLsp>,
}

impl ProjectGraphRuntimeOverlaySpanProvider {
    pub fn new(bex: Arc<dyn bex_project::BexLsp>) -> Self {
        Self { bex }
    }
}

impl GraphRuntimeOverlaySpanProvider for ProjectGraphRuntimeOverlaySpanProvider {
    fn cfg_node_spans_for_run(&self, run: &Run) -> GraphRuntimeOverlaySpanResolution {
        let Some(function_name) = overlay_function_name(run) else {
            return GraphRuntimeOverlaySpanResolution::Unavailable(project_generation_unavailable(
                run,
                "run target does not have a value-free control-flow graph",
            ));
        };
        let Some(graph) = self.bex.control_flow_graph_for_generation(
            &run.request.project_id.0,
            run.request.project_generation.0,
            function_name,
        ) else {
            return GraphRuntimeOverlaySpanResolution::Unavailable(project_generation_unavailable(
                run,
                "captured ProjectStore control-flow graph is unavailable",
            ));
        };

        GraphRuntimeOverlaySpanResolution::Available(
            graph
                .nodes
                .values()
                .filter_map(|node| {
                    let span = node.source_span.as_ref()?;
                    Some(CfgNodeSourceSpan {
                        cfg_node_id: CfgNodeId(u64::from(node.id.raw())),
                        file_id: u64::from(span.file_id),
                        start_offset: span.start_offset,
                        end_offset: span.end_offset,
                    })
                })
                .collect(),
        )
    }
}

fn overlay_function_name(run: &Run) -> Option<&str> {
    overlay_function_name_for_target(&run.target)
}

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

fn project_generation_unavailable(run: &Run, reason: &str) -> RunDiagnostic {
    RunDiagnostic {
        severity: DiagnosticSeverity::Info,
        code: Some("GraphOverlayProjectGenerationUnavailable".to_string()),
        message: format!(
            "Runtime graph overlay left call-site-provenance calls unattached because {reason} for project {} generation {}; no current-editor or function-name fallback was used.",
            run.request.project_id.0, run.request.project_generation.0
        ),
        call_node_id: None,
        payload_id: None,
    }
}

pub fn broadcast_run_patch(broadcast_tx: &broadcast::Sender<WsOutMessage>, patch: &RunPatch) {
    let _ = broadcast_tx.send(WsOutMessage::RunPatch {
        patch: patch_to_wire(patch),
    });
}

pub struct RunStoreProfileObserver {
    run_store: Arc<InMemoryRunStore>,
    broadcast_tx: broadcast::Sender<WsOutMessage>,
}

impl RunStoreProfileObserver {
    pub fn new(
        run_store: Arc<InMemoryRunStore>,
        broadcast_tx: broadcast::Sender<WsOutMessage>,
    ) -> Self {
        Self {
            run_store,
            broadcast_tx,
        }
    }
}

impl ProfileEventObserver for RunStoreProfileObserver {
    fn ingest_profile_event(&self, envelope: ProfileEventEnvelope) {
        for patch in self.run_store.ingest_profile_event(envelope) {
            broadcast_run_patch(&self.broadcast_tx, &patch);
        }
    }

    fn engine_closed(&self, engine_id: bex_events::ids::EngineId) {
        for patch in self.run_store.engine_closed(engine_id) {
            broadcast_run_patch(&self.broadcast_tx, &patch);
        }
    }
}

pub use bex_events::run::{patch_to_wire, run_summary_to_wire, run_to_wire};

#[cfg(test)]
mod tests {
    use bex_events::{
        ids::{BexCallId, BexThreadId, CallRef, EngineId, FunctionId, ProcessEuid},
        run::{
            AttachRootTraceResult, BoundaryId, ExecutionRequest, HeaderObservation, HostCallId,
            InMemoryRunStore, ProfileEvent, ProfileEventEnvelope, ProfileEventKind,
            ProfileEventObserver, ProfileEventSource, ProjectGeneration, ProjectId, RequestId,
            RunPatchChange, RunStatus, RunTarget, RuntimeTarget,
        },
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

    #[test]
    fn profile_observer_broadcasts_reconstruction_patches() {
        let (broadcast_tx, mut broadcast_rx) = broadcast::channel(8);
        let store = Arc::new(InMemoryRunStore::default());
        let observer = RunStoreProfileObserver::new(store.clone(), broadcast_tx);
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
        let AttachRootTraceResult::Attached { patches } = store.attach_root_trace(
            start.boundary_id,
            CallRef {
                process_euid: ProcessEuid([9; 16]),
                engine_id: EngineId(1),
                thread_id: BexThreadId(1),
                call_id: BexCallId(1),
            },
        ) else {
            panic!("root trace should attach");
        };
        assert!(patches.is_empty());

        ProfileEventObserver::ingest_profile_event(
            &observer,
            live_event(
                ProfileEventKind::StartThread {
                    thread_id: BexThreadId(1),
                    parent_thread_id: None,
                    parent_call_id: None,
                    name: None,
                },
                10,
            ),
        );
        assert!(broadcast_rx.try_recv().is_err());

        ProfileEventObserver::ingest_profile_event(
            &observer,
            live_event(
                ProfileEventKind::CallFunction {
                    thread_id: BexThreadId(1),
                    call_id: BexCallId(1),
                    parent_call_id: None,
                    function_id: FunctionId(1),
                    call_site_source: None,
                },
                20,
            ),
        );

        let WsOutMessage::RunPatch { patch } = broadcast_rx
            .try_recv()
            .expect("live reconstruction patch should broadcast")
        else {
            panic!("expected runPatch");
        };
        assert_eq!(patch["boundaryId"], start.boundary_id.to_wire_string());
        assert!(
            patch["changes"]
                .as_array()
                .unwrap()
                .iter()
                .any(|change| change["type"] == "upsertCallNode")
        );
        assert!(
            patch["changes"]
                .as_array()
                .unwrap()
                .iter()
                .any(|change| change["type"] == "setGraphRuntimeOverlay")
        );

        let snapshot_wire = run_to_wire(&store.snapshot(start.boundary_id).unwrap());
        let overlay = &snapshot_wire["graphRuntimeOverlay"];
        assert_eq!(overlay["boundaryId"], start.boundary_id.to_wire_string());
        assert_eq!(overlay["projectGeneration"], 1);
        assert_eq!(overlay["entries"].as_array().unwrap().len(), 0);
        assert_eq!(
            overlay["unattachedCallNodeIds"].as_array().unwrap().len(),
            1
        );
        assert_eq!(
            overlay["diagnostics"][0]["code"],
            "GraphOverlayCallSiteUnavailable"
        );
        assert_eq!(
            snapshot_wire["calls"][0]["callSiteSource"],
            serde_json::Value::Null
        );
        let encoded = serde_json::to_string(&snapshot_wire).unwrap();
        assert!(!encoded.contains("process_euid"));
        assert!(!encoded.contains("trace_key"));
    }

    fn live_event(kind: ProfileEventKind, timestamp_ns: u64) -> ProfileEventEnvelope {
        ProfileEventEnvelope {
            source: ProfileEventSource::Live {
                target: RuntimeTarget::Native,
                source_id: "test".to_string(),
            },
            process_euid: ProcessEuid([9; 16]),
            engine_id: EngineId(1),
            event: ProfileEvent { timestamp_ns, kind },
        }
    }
}
