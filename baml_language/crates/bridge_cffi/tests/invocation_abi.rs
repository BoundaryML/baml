//! Exercise the raw boundary independently of SDK code generation.
use std::{
    collections::HashMap,
    sync::{
        LazyLock, Mutex,
        atomic::{AtomicU32, Ordering},
    },
};

use bridge_cffi::baml_bridge::cffi::{
    BamlHandle, BamlHandleType, BamlOutboundResult, CallFunctionArgs, InboundMapEntry,
    InboundValue, InvocationOptions, TraceSelection, baml_outbound_result, baml_outbound_value,
    call_function_args::CallTarget, inbound_map_entry::Key, inbound_value::Value, trace_selection,
};
use bridge_ctypes::{
    CffiHandleTableEntry, HANDLE_TABLE, OwnedHostInvocation, TraceReservationHandle,
};
use prost::Message;

static DISPATCH: LazyLock<Mutex<Option<tokio::sync::mpsc::UnboundedSender<OwnedHostInvocation>>>> =
    LazyLock::new(Mutex::default);
static CANCELLED: AtomicU32 = AtomicU32::new(0);

extern "C" fn dispatch(request: *const u8, length: usize) {
    // SAFETY: the ABI lends these bytes for the callback duration.
    let bytes = unsafe { std::slice::from_raw_parts(request, length) };
    let frame = OwnedHostInvocation(
        bridge_cffi::invocation_protocol::decode_host_invocation(bytes).unwrap(),
    );
    DISPATCH
        .lock()
        .unwrap()
        .as_ref()
        .unwrap()
        .send(frame)
        .unwrap();
}
extern "C" fn cancelled(id: u32) {
    CANCELLED.store(id, Ordering::Release);
}

fn request(name: &str) -> CallFunctionArgs {
    CallFunctionArgs {
        call_id: bridge_cffi::new_function_call_id(),
        call_target: Some(CallTarget::FunctionName(name.into())),
        invocation: Some(InvocationOptions::default()),
        ..Default::default()
    }
}
fn handle_arg(key: u64) -> InboundMapEntry {
    InboundMapEntry {
        key: Some(Key::StringKey("unused".into())),
        value: Some(InboundValue {
            value: Some(Value::Handle(BamlHandle {
                key,
                handle_type: BamlHandleType::FunctionRef as i32,
            })),
            ..Default::default()
        }),
    }
}
fn assert_cancelled(bytes: &[u8]) {
    let envelope = BamlOutboundResult::decode(bytes).unwrap();
    let Some(baml_outbound_result::Result::Panic(panic)) = envelope.result else {
        panic!("expected cancellation: {envelope:?}")
    };
    let Some(baml_outbound_value::Value::ClassValue(class)) = panic.value.unwrap().value else {
        panic!("expected cancellation class")
    };
    assert_eq!(class.name, "baml.panics.Cancelled");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn raw_invocation_and_callback_lifecycles() {
    bridge_cffi::stage_runtime(".", HashMap::from([("main.baml".into(), "function identity() -> int throws never { 7 }\nfunction through_host(callback: () -> int) -> int { callback() }\nfunction reserve() -> trace.ReservedSpan throws never { trace.span().reserve() }".into())])).unwrap();

    let id = bridge_cffi::new_function_call_id();
    assert_ne!(id, 0);
    let now = bridge_cffi::invocation_clock_by_id(id).unwrap();
    assert!(bridge_cffi::invocation_clock_by_id(id).unwrap() >= now);
    assert!(bridge_cffi::release_function_call_by_id(id));
    assert!(!bridge_cffi::release_function_call_by_id(id));
    assert!(!bridge_cffi::cancel_function_call_by_id(id));
    assert!(bridge_cffi::invocation_clock_by_id(id).is_err());

    // Rejected legacy/control envelopes retire allocation and wire owners.
    let mut legacy = request("identity");
    legacy.invocation = None;
    let key = HANDLE_TABLE.insert(CffiHandleTableEntry::FunctionRef { global_index: 0 });
    legacy.kwargs.push(handle_arg(key));
    assert!(bridge_cffi::decode_invocation_request(&legacy.encode_to_vec()).is_err());
    assert!(HANDLE_TABLE.resolve(key).is_none());
    assert!(!bridge_cffi::cancel_function_call_by_id(legacy.call_id));

    let unknown = request("identity");
    let mut bytes = unknown.encode_to_vec();
    bytes.extend([0x38, 1]);
    assert!(bridge_cffi::decode_invocation_request(&bytes).is_err());
    assert!(!bridge_cffi::cancel_function_call_by_id(unknown.call_id));

    let mut malformed = request("identity");
    let key = HANDLE_TABLE.insert(CffiHandleTableEntry::FunctionRef { global_index: 0 });
    malformed.kwargs.push(handle_arg(key));
    let mut bytes = malformed.encode_to_vec();
    bytes.push(0xff); // Invalid trailing protobuf key after the allocation and owner.
    assert!(bridge_cffi::decode_invocation_request(&bytes).is_err());
    assert!(HANDLE_TABLE.resolve(key).is_none());
    assert!(!bridge_cffi::cancel_function_call_by_id(malformed.call_id));

    // Both control capability kinds reject ordinary, stale and foreign keys.
    let wrong = HANDLE_TABLE.insert(CffiHandleTableEntry::FunctionRef { global_index: 0 });
    let mut call = request("identity");
    call.invocation.as_mut().unwrap().inherited_state = wrong;
    assert!(bridge_cffi::decode_invocation_request(&call.encode_to_vec()).is_err());
    assert!(
        HANDLE_TABLE.resolve(wrong).is_some(),
        "controls borrow their keys"
    );
    let mut call = request("identity");
    call.invocation.as_mut().unwrap().trace = Some(TraceSelection {
        selection: Some(trace_selection::Selection::Reservation(wrong)),
    });
    assert!(bridge_cffi::decode_invocation_request(&call.encode_to_vec()).is_err());
    HANDLE_TABLE.release(wrong);

    let call = request("identity");
    assert!(bridge_cffi::cancel_function_call_by_id(call.call_id));
    let prepared = bridge_cffi::decode_invocation_request(&call.encode_to_vec()).unwrap();
    assert!(bridge_cffi::decode_invocation_request(&call.encode_to_vec()).is_err());
    assert_cancelled(&bridge_cffi::execute_invocation(prepared).await);

    // Real generated reservations are pinned by typed control capabilities.
    let reserve = request("reserve");
    let result = bridge_cffi::execute_invocation(
        bridge_cffi::decode_invocation_request(&reserve.encode_to_vec()).unwrap(),
    )
    .await;
    let envelope = BamlOutboundResult::decode(result.as_slice()).unwrap();
    let Some(baml_outbound_result::Result::Ok(value)) = envelope.result else {
        panic!("reserve failed: {envelope:?}")
    };
    let Some(baml_outbound_value::Value::ClassValue(class)) = &value.value else {
        panic!("expected ReservedSpan")
    };
    let handle = class
        .fields
        .iter()
        .find(|field| field.key == "_handle")
        .unwrap()
        .value
        .as_ref()
        .unwrap();
    let Some(baml_outbound_value::Value::HandleValue(handle)) = &handle.value else {
        panic!("expected reservation data")
    };
    let entry = HANDLE_TABLE.resolve(handle.key).unwrap();
    let CffiHandleTableEntry::RustData(data) = &*entry else {
        panic!("expected reservation RustData")
    };
    let reservation = data
        .0
        .clone()
        .downcast::<bex_project::ReservedSpanData>()
        .unwrap();
    let reservation_key = HANDLE_TABLE.insert(CffiHandleTableEntry::TraceReservation(
        TraceReservationHandle {
            owner: bridge_cffi::get_or_init_runtime().unwrap(),
            reservation,
        },
    ));
    bridge_ctypes::release_outbound_references(value);
    let mut cancelled_call = request("identity");
    cancelled_call.invocation.as_mut().unwrap().trace = Some(TraceSelection {
        selection: Some(trace_selection::Selection::Reservation(reservation_key)),
    });
    assert!(bridge_cffi::cancel_function_call_by_id(
        cancelled_call.call_id
    ));
    let prepared = bridge_cffi::decode_invocation_request(&cancelled_call.encode_to_vec()).unwrap();
    assert_cancelled(&bridge_cffi::execute_invocation(prepared).await);
    // Cancellation before admission did not consume it. The successful call does.
    let mut attached = request("identity");
    attached.invocation.as_mut().unwrap().trace =
        cancelled_call.invocation.as_ref().unwrap().trace.clone();
    let prepared = bridge_cffi::decode_invocation_request(&attached.encode_to_vec()).unwrap();
    let result = bridge_cffi::execute_invocation(prepared).await;
    assert!(matches!(
        BamlOutboundResult::decode(result.as_slice())
            .unwrap()
            .result,
        Some(baml_outbound_result::Result::Ok(_))
    ));
    let mut reused = request("identity");
    reused.invocation.as_mut().unwrap().trace = attached.invocation.as_ref().unwrap().trace.clone();
    let reused = bridge_cffi::decode_invocation_request(&reused.encode_to_vec()).unwrap();
    let reused = bridge_cffi::execute_invocation(reused).await;
    assert!(matches!(
        BamlOutboundResult::decode(reused.as_slice())
            .unwrap()
            .result,
        Some(baml_outbound_result::Result::Error(_))
    ));
    assert!(
        HANDLE_TABLE.resolve(reservation_key).is_some(),
        "reservation controls borrow ownership"
    );
    assert!(!bridge_cffi::cancel_function_call_by_id(call.call_id));
    let mut expired = request("identity");
    expired.invocation.as_mut().unwrap().deadline_ns = Some(0);
    let prepared = bridge_cffi::decode_invocation_request(&expired.encode_to_vec()).unwrap();
    assert_cancelled(&bridge_cffi::execute_invocation(prepared).await);

    let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel();
    *DISPATCH.lock().unwrap() = Some(sender);
    bridge_cffi::register_host_dispatch_v2(dispatch);
    bridge_cffi::register_host_cancel_callback(cancelled);
    let mut call = request("through_host");
    let parent_id = call.call_id;
    let controls = call.invocation.as_mut().unwrap();
    controls.host_environment = parent_id;
    let deadline = Some(bridge_cffi::invocation_clock_by_id(parent_id).unwrap() + 30_000_000_000);
    controls.deadline_ns = deadline;
    call.kwargs.push(InboundMapEntry {
        key: Some(Key::StringKey("callback".into())),
        value: Some(InboundValue {
            value: Some(Value::Handle(BamlHandle {
                key: 700001,
                handle_type: BamlHandleType::HostValueCallable as i32,
            })),
            ..Default::default()
        }),
    });
    let prepared = bridge_cffi::decode_invocation_request(&call.encode_to_vec()).unwrap();
    let parent = tokio::spawn(bridge_cffi::execute_invocation(prepared));
    let frame = tokio::time::timeout(std::time::Duration::from_secs(10), receiver.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(frame.0.host_environment, parent_id);
    assert_eq!(frame.0.deadline_ns, deadline);
    assert!(frame.0.cancel.is_some());
    assert!(matches!(
        &*HANDLE_TABLE.resolve(frame.0.effective_state).unwrap(),
        CffiHandleTableEntry::InvocationState(_)
    ));

    // Reentry borrows the effective frame, and inherits complete cancellation.
    let mut child = request("identity");
    child.invocation.as_mut().unwrap().inherited_state = frame.0.effective_state;
    let child = bridge_cffi::decode_invocation_request(&child.encode_to_vec()).unwrap();
    assert!(bridge_cffi::cancel_function_call_by_id(parent_id));
    assert_cancelled(&parent.await.unwrap());
    assert_cancelled(&bridge_cffi::execute_invocation(child).await);
    assert_eq!(CANCELLED.load(Ordering::Acquire), frame.0.callback_id);
    assert!(sys_native::host_dispatch::execution_origin(frame.0.callback_id).is_some());

    // A replacement runtime must reject a capability from the old one.
    let allocated_old = request("identity");
    bridge_cffi::stage_runtime(
        ".",
        HashMap::from([(
            "main.baml".into(),
            "function identity() -> int throws never { 99 }".into(),
        )]),
    )
    .unwrap();
    let mut foreign = request("identity");
    foreign.invocation.as_mut().unwrap().inherited_state = frame.0.effective_state;
    assert!(bridge_cffi::decode_invocation_request(&foreign.encode_to_vec()).is_err());
    let mut foreign = request("identity");
    foreign.invocation.as_mut().unwrap().trace =
        attached.invocation.as_ref().unwrap().trace.clone();
    assert!(bridge_cffi::decode_invocation_request(&foreign.encode_to_vec()).is_err());
    let old = bridge_cffi::decode_invocation_request(&allocated_old.encode_to_vec()).unwrap();
    let old = bridge_cffi::execute_invocation(old).await;
    // Replacement schedules shutdown of the previous runtime. The allocation
    // remains bound to it: it may finish there or report its shutdown, but may
    // never execute against the newly installed runtime (which returns 99).
    let old = BamlOutboundResult::decode(old.as_slice()).unwrap();
    match old.result {
        Some(baml_outbound_result::Result::Ok(value)) => {
            assert_eq!(value.value, Some(baml_outbound_value::Value::IntValue(7)));
        }
        Some(baml_outbound_result::Result::Error(error)) => {
            assert!(format!("{error:?}").contains("shutting down"), "{error:?}");
        }
        Some(baml_outbound_result::Result::Panic(panic)) => {
            assert!(format!("{panic:?}").contains("shutting down"), "{panic:?}");
        }
        other => panic!("unexpected original-runtime result: {other:?}"),
    }
    HANDLE_TABLE.release(reservation_key);

    // Late completion reports actual exit and releases its transferred value.
    let key = HANDLE_TABLE.insert(CffiHandleTableEntry::FunctionRef { global_index: 0 });
    let late = InboundValue {
        value: Some(Value::Handle(BamlHandle {
            key,
            handle_type: BamlHandleType::FunctionRef as i32,
        })),
        ..Default::default()
    }
    .encode_to_vec();
    bridge_cffi::complete_host_call(frame.0.callback_id, 0, late.as_ptr().cast(), late.len());
    assert!(HANDLE_TABLE.resolve(key).is_none());
    assert!(sys_native::host_dispatch::execution_origin(frame.0.callback_id).is_none());
    let state = frame.0.effective_state;
    drop(frame);
    assert!(HANDLE_TABLE.resolve(state).is_none());
    *DISPATCH.lock().unwrap() = None;
    bridge_cffi::shutdown_runtime(None).await.unwrap();
}
