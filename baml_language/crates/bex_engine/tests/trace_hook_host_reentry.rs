//! Only a host test can cross the V2 callback ABI and re-enter a new VM with
//! its effective inherited state while the selecting hook is still suspended.
#![cfg(not(target_arch = "wasm32"))]
#![allow(unsafe_code)]

mod common;

use std::{
    sync::{Arc, OnceLock},
    time::Duration,
};

use bex_engine::{BexEngine, BexExternalValue, CallId, FunctionCallContextBuilder};
use bex_resource_types::{HostValueArc, HostValueKind};
use bridge_ctypes::{
    CffiHandleTableEntry, HANDLE_TABLE, OwnedHostInvocation, baml_bridge::cffi::HostInvocation,
};
use prost::Message;
use sys_native::SysOpsExt;

static CALLBACKS: OnceLock<tokio::sync::mpsc::UnboundedSender<OwnedHostInvocation>> =
    OnceLock::new();

extern "C" fn dispatch(request: *const u8, length: usize) {
    // SAFETY: the ABI keeps the request slice alive until this dispatch returns.
    let bytes = unsafe { std::slice::from_raw_parts(request, length) };
    let frame = OwnedHostInvocation(HostInvocation::decode(bytes).unwrap());
    CALLBACKS.get().unwrap().send(frame).unwrap();
}

const SOURCE: &str = r#"
class State { selections int child_selections int bodies int callbacks int }
function make_state() -> State { State { selections: 0, child_selections: 0, bodies: 0, callbacks: 0 } }
function select_child(state: State, settings: trace.Settings) -> trace.Options throws never {
    state.child_selections += 1;
    trace.context(metadata = { "child_selected": true })
}
/// baml:$trace=select_child
function child(state: State) -> int {
    state.callbacks += 1;
    let phase = trace.current_context().metadata["phase"];
    if (phase == "hook") {
        assert.equal(state.child_selections, 0);
    } else if (phase == "body") {
        assert.equal(state.child_selections, 1);
        assert.equal(trace.current_context().metadata["child_selected"], true);
    } else {
        assert.equal(phase, "after");
        assert.equal(state.child_selections, 2);
    }
    7
}
function select(callback: (State) -> int throws never, state: State, settings: trace.Settings) -> trace.Options throws never {
    state.selections += 1;
    let invoke = () => { callback(state) };
    assert.equal(invoke($trace = trace.context(metadata = { "phase": "hook" })), 7);
    trace.context(metadata = { "phase": "body" })
}
/// baml:$trace=select
function target(callback: (State) -> int throws never, state: State) -> int {
    state.bodies += 1;
    assert.equal(callback(state), 7);
    assert.equal(state.selections, 1);
    assert.equal(state.child_selections, 1);
    assert.equal(state.bodies, 1);
    assert.equal(state.callbacks, 2);
    42
}
"#;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn host_reentry_inherits_suppression_and_context_then_restores_selection() {
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    CALLBACKS.set(tx).unwrap();
    sys_native::host_dispatch::set_dispatch_v2(dispatch);
    let engine = Arc::new(
        BexEngine::new(
            common::compile_for_engine(SOURCE),
            Arc::new(sys_native::SysOps::native()),
            vec![],
        )
        .unwrap(),
    );
    let state = engine
        .call_function(
            "make_state",
            vec![],
            FunctionCallContextBuilder::new(CallId::next()).build(),
            false,
        )
        .await
        .unwrap();
    assert!(
        matches!(state, BexExternalValue::Handle(_)),
        "share the live state across both VMs"
    );

    let host = tokio::spawn({
        let engine = engine.clone();
        let state = state.clone();
        async move {
            for phase in ["hook", "body"] {
                let frame = rx.recv().await.unwrap();
                assert_eq!(frame.0.host_environment, 41);
                let entry = HANDLE_TABLE.resolve(frame.0.effective_state).unwrap();
                let CffiHandleTableEntry::InvocationState(inherited) = &*entry else {
                    panic!("typed effective state")
                };
                let inherited = inherited.state.clone();
                assert_eq!(
                    inherited.trace_context().metadata()["phase"],
                    btel_types::context::ContextValue::String(phase.into())
                );
                let value = engine
                    .call_function(
                        "child",
                        vec![state.clone()],
                        FunctionCallContextBuilder::new(CallId::next())
                            .with_inherited_state(inherited)
                            .build(),
                        true,
                    )
                    .await
                    .unwrap();
                assert_eq!(value, BexExternalValue::Int(7));
                // Complete the parked BAML call only after its re-entrant child finishes.
                sys_native::host_dispatch::complete_with_value(frame.0.callback_id, value);
            }
        }
    });
    let value = tokio::time::timeout(
        Duration::from_secs(10),
        engine.call_function(
            "target",
            vec![
                BexExternalValue::HostValue(HostValueArc::new(1, HostValueKind::Callable)),
                state.clone(),
            ],
            FunctionCallContextBuilder::new(CallId::next())
                .with_host_environment(41)
                .build(),
            true,
        ),
    )
    .await
    .expect("host re-entry must not deadlock")
    .unwrap();
    assert_eq!(value, BexExternalValue::Int(42));
    host.await.unwrap();
    assert_eq!(
        engine
            .call_function(
                "child",
                vec![state],
                FunctionCallContextBuilder::new(CallId::next())
                    .with_trace_options(bex_vm_types::trace::TraceOptionsData {
                        context: Some(Arc::new(btel_types::context::ContextPatch {
                            metadata: [(
                                "phase".into(),
                                Some(btel_types::context::ContextValue::String("after".into()))
                            )]
                            .into(),
                            ..Default::default()
                        })),
                        ..Default::default()
                    })
                    .build(),
                true
            )
            .await
            .unwrap(),
        BexExternalValue::Int(7)
    );
    tokio::time::timeout(Duration::from_secs(5), engine.shutdown())
        .await
        .expect("callback controls must not retain execution");
}
