//! Exercise the encoded request path shared by the native SDK adapters.
use std::{collections::HashMap, sync::Arc};

use bex_project::{Bex, FsPath, RuntimeTy};
use bridge_cffi::{invoke_prepared, invoke_prepared_encoded, prepare_call};
use bridge_ctypes::{
    HANDLE_TABLE, TransferError, TransferSession,
    baml_bridge::cffi::{
        BamlHandle, BamlHandleType, BamlOutboundHandle, BamlOutboundResult, BamlOutboundValue,
        BamlTyArg, CallFunctionArgs, ConcreteMethodTarget, InboundMapEntry, InboundValue,
        InterfaceMethodTarget, baml_outbound_result, baml_outbound_value,
        call_function_args::CallTarget, inbound_map_entry::Key, inbound_value::Value,
    },
    runtime_ty_to_proto_ty,
};
use prost::Message;
use sys_native::SysOpsExt;

static RELEASED: std::sync::Mutex<Vec<u64>> = std::sync::Mutex::new(Vec::new());

extern "C" fn capture_release(key: u64) {
    RELEASED.lock().unwrap().push(key);
}

#[test]
fn last_owned_host_reference_delivers_release_without_another_vm_call() {
    use bridge_ctypes::CffiHandleTableEntry;
    bex_project::host_release_dispatch::install(capture_release).unwrap();
    let host = bex_project::HostValueArc::new(12345678, bex_project::HostValueKind::Opaque);
    let weak = Arc::downgrade(&host);
    let key = HANDLE_TABLE.insert(CffiHandleTableEntry::HostValue(host));
    let cloned = bridge_cffi::handle::clone_handle(key).unwrap();
    assert_eq!(
        unsafe { bridge_cffi::baml_handle_release(key) },
        bridge_cffi::BamlCffiStatus::Ok
    );
    assert!(RELEASED.lock().unwrap().is_empty());
    assert!(weak.upgrade().is_some());
    assert_eq!(
        unsafe { bridge_cffi::baml_handle_release(cloned) },
        bridge_cffi::BamlCffiStatus::Ok
    );
    assert!(weak.upgrade().is_none());
    assert_eq!(*RELEASED.lock().unwrap(), vec![12345678]);
    assert_eq!(
        unsafe { bridge_cffi::baml_handle_release(cloned) },
        bridge_cffi::BamlCffiStatus::InvalidHandle
    );
    assert_eq!(*RELEASED.lock().unwrap(), vec![12345678]);
}

#[test]
fn resource_cleanup_panic_stays_inside_handle_release_abi() {
    struct PanicOnDrop;
    impl Drop for PanicOnDrop {
        fn drop(&mut self) {
            panic!("synthetic resource cleanup failure");
        }
    }
    let key = HANDLE_TABLE.insert(
        bridge_ctypes::CffiHandleTableEntry::try_from(bex_project::BexExternalValue::RustData(
            Arc::new(PanicOnDrop),
        ))
        .unwrap(),
    );
    assert_eq!(
        unsafe { bridge_cffi::baml_handle_release(key) },
        bridge_cffi::BamlCffiStatus::InternalError,
    );
    assert!(HANDLE_TABLE.resolve(key).is_none());
}

#[test]
fn rejected_preparation_releases_host_registrations_without_entering_the_engine() {
    bex_project::host_release_dispatch::install(capture_release).unwrap();
    let mut call = request(
        CallTarget::FunctionName("unused".into()),
        vec![
            (
                "first",
                InboundValue {
                    value: Some(Value::Handle(BamlHandle {
                        key: 111,
                        handle_type: BamlHandleType::HostValueCallable as i32,
                    })),
                    ..Default::default()
                },
            ),
            (
                "second",
                InboundValue {
                    value: Some(Value::Handle(BamlHandle {
                        key: 222,
                        handle_type: BamlHandleType::HostValueOpaque as i32,
                    })),
                    ..Default::default()
                },
            ),
        ],
    );
    call.call_id = 0;
    assert!(prepare_call(&call.encode_to_vec()).is_err());
    let mut released = RELEASED.lock().unwrap().clone();
    released.sort_unstable();
    assert_eq!(released, vec![111, 222]);
}

const SOURCE: &str = r#"
interface Counter {
    function add(self, amount: int, scale: int = 1) -> int throws never {
        self.current() + amount * scale
    }
    function current(self) -> int throws never
    function echo<T>(self, value: T) -> T throws never
    function twice(self, amount: int) -> int throws never {
        self.add(amount);
        self.add(amount)
    }
}
class StoredCounter {
    count: int,
    function reset(self, count: int = 0) -> int throws never { self.count = count; self.count }
    implements Counter {
        function add(self, delta: int, extra: int = 0, scale: int = 1) -> int throws never {
            self.count += delta * scale + extra;
            self.count
        }
        function current(self) -> int throws never { self.count }
        function echo<T>(self, value: T) -> T throws never { self.count += 1; value }
    }
}
function make() -> Counter throws never { StoredCounter { count: 0 } }
function make_concrete() -> StoredCounter throws never { StoredCounter { count: 0 } }
function counter_type() -> reflect.Type { reflect.Type.of<Counter>() }
function accept_type(counter: Counter, ty: reflect.Type) -> int throws never { counter.add(1) }
function identity<T>(value: T) -> T throws never { value }
function format(text: string, suffix: string = "!") -> string throws never { text + suffix }
function callable() -> (text: string, suffix?: string) -> string throws never { format }
interface Source {
    type Output
    function value(self) -> Self.Output throws never
    function ty(self) -> reflect.Type throws never
}
type Factory = () -> unknown throws never
function package_factory() -> Factory {
    let package = reflect.Package.compile({ "pair.baml": `
interface Named { function label(self) -> string throws never }
class Stored {
    implements Named { function label(self) -> string throws never { "named" } }
    implements app.Source {
        type Output = Named
        function value(self) -> Named throws never { self }
        function ty(self) -> reflect.Type throws never { reflect.Type.of<Named>() }
    }
}
function make() -> app.Source<Output=Named> throws never { Stored {} }
` }, packages = { "app": reflect.Package.current() })
    package.get_function<Factory>("root.make") ?? throw "missing factory"
}
"#;

fn runtime() -> Arc<dyn Bex> {
    bex_project::new(
        vfs::MemoryFS::new().into(),
        bex_project::SysOps::native(),
        HashMap::from([(FsPath::from_str("main.baml".into()), SOURCE.into())]),
    )
    .unwrap()
}

fn request(target: CallTarget, kwargs: Vec<(&str, InboundValue)>) -> CallFunctionArgs {
    CallFunctionArgs {
        call_id: sys_types::CallId::next().0,
        call_target: Some(target),
        kwargs: kwargs
            .into_iter()
            .map(|(name, value)| InboundMapEntry {
                key: Some(Key::StringKey(name.into())),
                value: Some(value),
            })
            .collect(),
        type_args: vec![],
    }
}

fn method(view: u64, member: &str, type_args: Vec<BamlTyArg>) -> CallTarget {
    CallTarget::InterfaceMethod(InterfaceMethodTarget {
        view,
        member: member.into(),
        type_args,
    })
}

fn concrete_method(receiver: u64, member: &str, type_args: Vec<BamlTyArg>) -> CallTarget {
    CallTarget::ConcreteMethod(ConcreteMethodTarget {
        receiver,
        class_name: "user.StoredCounter".into(),
        dispatch: Some(
            bridge_ctypes::baml_bridge::cffi::concrete_method_target::Dispatch::InterfacePattern(
                runtime_ty_to_proto_ty(&RuntimeTy::Interface(
                    bex_project::TypeName::from_dotted_path("user.Counter"),
                    vec![],
                    vec![],
                    bex_project::TyAttr::default(),
                )),
            ),
        ),
        member: member.into(),
        type_args,
    })
}

fn int(value: i64) -> InboundValue {
    InboundValue {
        value: Some(Value::IntValue(value)),
        ..Default::default()
    }
}

fn string(value: &str) -> InboundValue {
    InboundValue {
        value: Some(Value::StringValue(value.into())),
        ..Default::default()
    }
}

// Ordinary value arguments still use the current transfer lane. Clone the
// SDK ownership before sending it; method targets/type references are borrowed.
fn argument(handle: &BamlOutboundHandle) -> InboundValue {
    InboundValue {
        value: Some(Value::Handle(BamlHandle {
            key: HANDLE_TABLE.clone_handle(handle.key).unwrap(),
            handle_type: handle.handle_type,
        })),
        ..Default::default()
    }
}

async fn invoke(runtime: &Arc<dyn Bex>, request: CallFunctionArgs) -> BamlOutboundResult {
    let prepared = prepare_call(&request.encode_to_vec()).unwrap();
    BamlOutboundResult::decode(invoke_prepared(runtime.clone(), prepared).await.as_slice()).unwrap()
}

fn ok(result: BamlOutboundResult) -> BamlOutboundValue {
    match result.result {
        Some(baml_outbound_result::Result::Ok(value)) => value,
        other => panic!("expected success, got {other:?}"),
    }
}

fn handle(value: BamlOutboundValue, role: BamlHandleType) -> BamlOutboundHandle {
    let Some(baml_outbound_value::Value::HandleValue(handle)) = value.value else {
        panic!("expected handle, got {value:?}")
    };
    assert_eq!(handle.handle_type, role as i32);
    handle
}

fn integer(result: BamlOutboundResult, expected: i64) {
    assert_eq!(
        ok(result).value,
        Some(baml_outbound_value::Value::IntValue(expected))
    );
}

fn type_error(result: BamlOutboundResult) {
    let Some(baml_outbound_result::Result::Error(error)) = result.result else {
        panic!("expected boundary type error, got {result:?}")
    };
    let Some(baml_outbound_value::Value::ClassValue(value)) = error.value.unwrap().value else {
        panic!("expected typed error")
    };
    assert_eq!(value.name, "baml.errors.TypeMismatch");
}

async fn make(runtime: &Arc<dyn Bex>, name: &str, role: BamlHandleType) -> BamlOutboundHandle {
    handle(
        ok(invoke(
            runtime,
            request(CallTarget::FunctionName(name.into()), vec![]),
        )
        .await),
        role,
    )
}

#[tokio::test]
async fn staged_interface_results_survive_adoption_and_release_on_failed_delivery() {
    let runtime = runtime();
    let session = TransferSession::new(&HANDLE_TABLE);
    for _ in 0..8 {
        let call = request(CallTarget::FunctionName("make".into()), vec![]);
        let prepared = prepare_call(&call.encode_to_vec()).unwrap();
        let encoded = invoke_prepared_encoded(runtime.clone(), prepared).await;
        let delivery = session.stage(encoded).unwrap();
        let result = BamlOutboundResult::decode(delivery.payload().as_slice()).unwrap();
        let view = handle(ok(result), BamlHandleType::AdtInterface);
        let weak = Arc::downgrade(&HANDLE_TABLE.resolve(view.key).unwrap());
        assert!(weak.upgrade().is_some());
        drop(delivery); // no SDK callback/consumer accepted this result
        assert!(weak.upgrade().is_none());
        assert!(HANDLE_TABLE.resolve(view.key).is_none());
        assert_eq!(session.pending_count(), 0);
    }

    let call = request(CallTarget::FunctionName("make".into()), vec![]);
    let prepared = prepare_call(&call.encode_to_vec()).unwrap();
    let delivery = session
        .stage(invoke_prepared_encoded(runtime.clone(), prepared).await)
        .unwrap();
    let (receipt, bytes) = delivery.handoff();
    let view = handle(
        ok(BamlOutboundResult::decode(bytes.as_slice()).unwrap()),
        BamlHandleType::AdtInterface,
    );
    session.adopt(receipt, &[view.key]).unwrap();
    session.discard(receipt).unwrap(); // duplicate cleanup cannot revoke SDK ownership
    integer(
        invoke(
            &runtime,
            request(method(view.key, "add", vec![]), vec![("amount", int(3))]),
        )
        .await,
        3,
    );
    assert!(HANDLE_TABLE.release(view.key));
    assert_eq!(session.pending_count(), 0);

    // The receipt remains usable for cleanup when decoding the value bytes
    // fails before the host has discovered even one nested reference.
    let call = request(CallTarget::FunctionName("make".into()), vec![]);
    let prepared = prepare_call(&call.encode_to_vec()).unwrap();
    let delivery = session
        .stage(invoke_prepared_encoded(runtime.clone(), prepared).await)
        .unwrap();
    let view = handle(
        ok(BamlOutboundResult::decode(delivery.payload().as_slice()).unwrap()),
        BamlHandleType::AdtInterface,
    );
    let (receipt, _) = delivery.handoff();
    assert!(BamlOutboundResult::decode([0xff_u8].as_slice()).is_err());
    session.discard(receipt).unwrap();
    assert!(HANDLE_TABLE.resolve(view.key).is_none());
    assert_eq!(
        session.adopt(receipt, &[view.key]),
        Err(TransferError::NotPending)
    );
    session.close();
    runtime.shutdown().await;
}

#[tokio::test]
async fn singleton_replacement_and_shutdown_close_the_issuing_transfer_session() {
    let first =
        bridge_cffi::initialize_runtime(".", HashMap::from([("main.baml".into(), SOURCE.into())]))
            .unwrap();
    let (snapshot, first_session) = bridge_cffi::get_runtime_with_transfers().unwrap();
    assert!(Arc::ptr_eq(&first, &snapshot));
    let prepared =
        prepare_call(&request(CallTarget::FunctionName("make".into()), vec![]).encode_to_vec())
            .unwrap();
    let delivery = first_session
        .stage(invoke_prepared_encoded(first.clone(), prepared).await)
        .unwrap();
    let view = handle(
        ok(BamlOutboundResult::decode(delivery.payload().as_slice()).unwrap()),
        BamlHandleType::AdtInterface,
    );
    let receipt = delivery.receipt();
    assert!(HANDLE_TABLE.resolve(view.key).is_some());

    bridge_cffi::initialize_runtime(".", HashMap::from([("main.baml".into(), SOURCE.into())]))
        .unwrap();
    let (second, second_session) = bridge_cffi::get_runtime_with_transfers().unwrap();
    assert!(!Arc::ptr_eq(&first, &second));
    assert_eq!(first_session.pending_count(), 0);
    assert!(HANDLE_TABLE.resolve(view.key).is_none());
    assert_eq!(
        first_session.adopt(receipt, &[view.key]),
        Err(TransferError::Closed)
    );
    assert_eq!(
        second_session.adopt(receipt, &[view.key]),
        Err(TransferError::WrongSession)
    );
    drop(delivery);

    let prepared =
        prepare_call(&request(CallTarget::FunctionName("make".into()), vec![]).encode_to_vec())
            .unwrap();
    let delivery = second_session
        .stage(invoke_prepared_encoded(second, prepared).await)
        .unwrap();
    let view = handle(
        ok(BamlOutboundResult::decode(delivery.payload().as_slice()).unwrap()),
        BamlHandleType::AdtInterface,
    );
    bridge_cffi::shutdown_runtime().await.unwrap();
    assert!(bridge_cffi::get_runtime_with_transfers().is_err());
    assert_eq!(second_session.pending_count(), 0);
    assert!(HANDLE_TABLE.resolve(view.key).is_none());
    drop(delivery);
}

#[tokio::test]
async fn checked_methods_keep_prepared_receivers_and_types_alive() {
    let baseline = HANDLE_TABLE.len();
    let runtime = runtime();
    let counter = make(&runtime, "make", BamlHandleType::AdtInterface).await;
    let reflected = make(&runtime, "counter_type", BamlHandleType::AdtType).await;

    // Parameter names come from the interface, despite the implementation's
    // different required name and additional/reordered optional parameters.
    integer(
        invoke(
            &runtime,
            request(
                method(counter.key, "add", vec![]),
                vec![("scale", int(3)), ("amount", int(2))],
            ),
        )
        .await,
        6,
    );
    integer(
        invoke(
            &runtime,
            request(
                method(counter.key, "twice", vec![]),
                vec![("amount", int(2))],
            ),
        )
        .await,
        10,
    );
    for args in [
        vec![],
        vec![("delta", int(3))],
        vec![("amount", int(3)), ("extra", int(7))],
    ] {
        type_error(invoke(&runtime, request(method(counter.key, "add", vec![]), args)).await);
    }
    integer(
        invoke(
            &runtime,
            request(method(counter.key, "current", vec![]), vec![]),
        )
        .await,
        10,
    );

    let sdk_copy = HANDLE_TABLE.clone_handle(counter.key).unwrap();
    let call = request(
        method(
            sdk_copy,
            "echo",
            vec![BamlTyArg {
                type_reference: Some(reflected.key),
                ..Default::default()
            }],
        ),
        vec![("value", argument(&counter))],
    );
    let prepared = prepare_call(&call.encode_to_vec()).unwrap();
    assert!(HANDLE_TABLE.release(sdk_copy));
    assert!(HANDLE_TABLE.release(reflected.key));
    assert!(HANDLE_TABLE.resolve(sdk_copy).is_none());
    assert!(HANDLE_TABLE.resolve(reflected.key).is_none());
    let returned = handle(
        ok(
            BamlOutboundResult::decode(invoke_prepared(runtime.clone(), prepared).await.as_slice())
                .unwrap(),
        ),
        BamlHandleType::AdtInterface,
    );
    integer(
        invoke(
            &runtime,
            request(method(returned.key, "current", vec![]), vec![]),
        )
        .await,
        11,
    );
    assert!(HANDLE_TABLE.release(returned.key));

    // A named generic function accepts the same live evidence lane.
    let reflected = make(&runtime, "counter_type", BamlHandleType::AdtType).await;
    let mut call = request(
        CallTarget::FunctionName("identity".into()),
        vec![("value", argument(&counter))],
    );
    call.type_args.push(BamlTyArg {
        type_var: "T".into(),
        type_reference: Some(reflected.key),
        ..Default::default()
    });
    let prepared = prepare_call(&call.encode_to_vec()).unwrap();
    assert!(HANDLE_TABLE.release(reflected.key));
    let returned = handle(
        ok(
            BamlOutboundResult::decode(invoke_prepared(runtime.clone(), prepared).await.as_slice())
                .unwrap(),
        ),
        BamlHandleType::AdtInterface,
    );
    assert!(HANDLE_TABLE.release(returned.key));

    // A plain callable also pins its target before executor scheduling.
    let callable = make(&runtime, "callable", BamlHandleType::FunctionRef).await;
    let prepared = prepare_call(
        &request(
            CallTarget::FunctionHandle(callable.key),
            vec![("text", string("Ada"))],
        )
        .encode_to_vec(),
    )
    .unwrap();
    assert!(HANDLE_TABLE.release(callable.key));
    let result = ok(BamlOutboundResult::decode(
        invoke_prepared(runtime.clone(), prepared).await.as_slice(),
    )
    .unwrap());
    assert_eq!(
        result.value,
        Some(baml_outbound_value::Value::StringValue("Ada!".into()))
    );

    assert!(HANDLE_TABLE.release(counter.key));
    // If scheduling is abandoned altogether, dropping the prepared call must
    // release its receiver too; a runtime registry must not become its owner.
    let abandoned = make(&runtime, "make", BamlHandleType::AdtInterface).await;
    let weak_view = {
        let entry = HANDLE_TABLE.resolve(abandoned.key).unwrap();
        let bridge_ctypes::CffiHandleTableEntry::Adt(bex_project::BexExternalAdt::Interface(view)) =
            &*entry
        else {
            panic!("expected checked view")
        };
        Arc::downgrade(view)
    };
    let prepared =
        prepare_call(&request(method(abandoned.key, "current", vec![]), vec![]).encode_to_vec())
            .unwrap();
    assert!(HANDLE_TABLE.release(abandoned.key));
    assert!(weak_view.upgrade().is_some());
    drop(prepared);
    assert!(weak_view.upgrade().is_none());
    // Keep the runtime open: cleanup cannot depend on singleton shutdown.
    assert_eq!(HANDLE_TABLE.len(), baseline);
    runtime.shutdown().await;
}

#[tokio::test]
async fn foreign_type_references_fail_before_receiver_execution() {
    let baseline = HANDLE_TABLE.len();
    let receiving = runtime();
    let foreign = runtime();
    let counter = make(&receiving, "make", BamlHandleType::AdtInterface).await;
    let reflected = make(&foreign, "counter_type", BamlHandleType::AdtType).await;
    type_error(
        invoke(
            &receiving,
            request(
                method(
                    counter.key,
                    "echo",
                    vec![BamlTyArg {
                        type_reference: Some(reflected.key),
                        ..Default::default()
                    }],
                ),
                vec![("value", argument(&counter))],
            ),
        )
        .await,
    );
    type_error(
        invoke(
            &receiving,
            request(
                CallTarget::FunctionName("accept_type".into()),
                vec![
                    ("counter", argument(&counter)),
                    ("ty", argument(&reflected)),
                ],
            ),
        )
        .await,
    );
    integer(
        invoke(
            &receiving,
            request(method(counter.key, "current", vec![]), vec![]),
        )
        .await,
        0,
    );

    // Wrong role, a closed reference, and ambiguous type evidence fail during
    // preparation, before values or the implementing method can be evaluated.
    for arg in [
        BamlTyArg {
            type_reference: Some(counter.key),
            ..Default::default()
        },
        BamlTyArg {
            type_reference: Some(0),
            ..Default::default()
        },
        BamlTyArg {
            type_reference: Some(reflected.key),
            type_value: Some(runtime_ty_to_proto_ty(&RuntimeTy::int())),
            ..Default::default()
        },
        BamlTyArg::default(),
    ] {
        let before = HANDLE_TABLE.len();
        assert!(
            prepare_call(
                &request(
                    method(counter.key, "echo", vec![arg]),
                    vec![("value", argument(&counter)), ("later", argument(&counter)),]
                )
                .encode_to_vec()
            )
            .is_err()
        );
        assert_eq!(
            HANDLE_TABLE.len(),
            before,
            "early type rejection must release all argument transfers"
        );
    }
    for missing_target in [false, true] {
        let before = HANDLE_TABLE.len();
        let mut malformed = request(
            CallTarget::FunctionName("make".into()),
            vec![("unused", argument(&counter))],
        );
        if missing_target {
            malformed.call_target = None;
        } else {
            malformed.call_id = 0;
        }
        assert!(prepare_call(&malformed.encode_to_vec()).is_err());
        assert_eq!(HANDLE_TABLE.len(), before);
    }
    assert!(
        prepare_call(&request(method(reflected.key, "current", vec![]), vec![]).encode_to_vec())
            .is_err()
    );
    assert!(HANDLE_TABLE.release(reflected.key));
    assert!(HANDLE_TABLE.release(counter.key));
    assert!(
        prepare_call(&request(method(counter.key, "current", vec![]), vec![]).encode_to_vec())
            .is_err()
    );
    assert_eq!(HANDLE_TABLE.len(), baseline);
    receiving.shutdown().await;
    foreign.shutdown().await;
}

#[tokio::test]
async fn runtime_created_associated_types_keep_identity_through_encoded_calls() {
    let baseline = HANDLE_TABLE.len();
    let runtime = runtime();
    let counter = make(&runtime, "make", BamlHandleType::AdtInterface).await;
    async fn source(runtime: &Arc<dyn Bex>) -> (BamlOutboundHandle, BamlOutboundHandle) {
        let factory = make(runtime, "package_factory", BamlHandleType::FunctionRef).await;
        let source = handle(
            ok(invoke(
                runtime,
                request(CallTarget::FunctionHandle(factory.key), vec![]),
            )
            .await),
            BamlHandleType::AdtInterface,
        );
        assert!(HANDLE_TABLE.release(factory.key));
        let ty = handle(
            ok(invoke(runtime, request(method(source.key, "ty", vec![]), vec![])).await),
            BamlHandleType::AdtType,
        );
        let item = handle(
            ok(invoke(
                runtime,
                request(method(source.key, "value", vec![]), vec![]),
            )
            .await),
            BamlHandleType::AdtInterface,
        );
        assert!(HANDLE_TABLE.release(source.key));
        (ty, item)
    }
    let (first_type, first_value) = source(&runtime).await;
    let (second_type, second_value) = source(&runtime).await;
    // Both compilations spell their interface user.Named. Display names must
    // not replace the declarations retained in these two type capabilities.
    type_error(
        invoke(
            &runtime,
            request(
                method(
                    counter.key,
                    "echo",
                    vec![BamlTyArg {
                        type_reference: Some(first_type.key),
                        ..Default::default()
                    }],
                ),
                vec![("value", argument(&second_value))],
            ),
        )
        .await,
    );
    integer(
        invoke(
            &runtime,
            request(method(counter.key, "current", vec![]), vec![]),
        )
        .await,
        0,
    );

    let prepared = prepare_call(
        &request(
            method(
                counter.key,
                "echo",
                vec![BamlTyArg {
                    type_reference: Some(first_type.key),
                    ..Default::default()
                }],
            ),
            vec![("value", argument(&first_value))],
        )
        .encode_to_vec(),
    )
    .unwrap();
    assert!(HANDLE_TABLE.release(first_type.key));
    assert!(HANDLE_TABLE.release(first_value.key));
    let returned = handle(
        ok(
            BamlOutboundResult::decode(invoke_prepared(runtime.clone(), prepared).await.as_slice())
                .unwrap(),
        ),
        BamlHandleType::AdtInterface,
    );
    let label = ok(invoke(
        &runtime,
        request(method(returned.key, "label", vec![]), vec![]),
    )
    .await);
    assert_eq!(
        label.value,
        Some(baml_outbound_value::Value::StringValue("named".into()))
    );
    integer(
        invoke(
            &runtime,
            request(method(counter.key, "current", vec![]), vec![]),
        )
        .await,
        1,
    );
    for key in [counter.key, returned.key, second_type.key, second_value.key] {
        assert!(HANDLE_TABLE.release(key));
    }
    assert_eq!(HANDLE_TABLE.len(), baseline);
    runtime.shutdown().await;
}

#[tokio::test]
async fn concrete_method_wire_target_retains_receiver_and_uses_implementation_contract() {
    let runtime = runtime();
    let counter = make(&runtime, "make_concrete", BamlHandleType::ConcreteObject).await;
    integer(
        invoke(
            &runtime,
            request(
                concrete_method(counter.key, "add", vec![]),
                vec![("delta", int(2)), ("extra", int(1)), ("scale", int(3))],
            ),
        )
        .await,
        7,
    );
    type_error(
        invoke(
            &runtime,
            request(
                concrete_method(counter.key, "add", vec![]),
                vec![("amount", int(99))],
            ),
        )
        .await,
    );
    let mut wrong_class = concrete_method(counter.key, "current", vec![]);
    if let CallTarget::ConcreteMethod(target) = &mut wrong_class {
        target.class_name = "user.Source".into();
    }
    type_error(invoke(&runtime, request(wrong_class, vec![])).await);
    let prepared = prepare_call(
        &request(concrete_method(counter.key, "current", vec![]), vec![]).encode_to_vec(),
    )
    .unwrap();
    assert!(HANDLE_TABLE.release(counter.key));
    integer(
        BamlOutboundResult::decode(invoke_prepared(runtime.clone(), prepared).await.as_slice())
            .unwrap(),
        7,
    );
}

#[tokio::test]
async fn concrete_method_preparation_rejection_releases_the_entire_argument_batch() {
    let runtime = runtime();
    let counter = make(&runtime, "make_concrete", BamlHandleType::ConcreteObject).await;
    let view = make(&runtime, "make", BamlHandleType::AdtInterface).await;
    let mut invalid = concrete_method(counter.key, "current", vec![]);
    if let CallTarget::ConcreteMethod(target) = &mut invalid {
        target.dispatch = None;
    }
    let mut false_inherent = concrete_method(counter.key, "reset", vec![]);
    if let CallTarget::ConcreteMethod(target) = &mut false_inherent {
        target.dispatch = Some(
            bridge_ctypes::baml_bridge::cffi::concrete_method_target::Dispatch::Inherent(false),
        );
    }
    for target in [
        concrete_method(view.key, "current", vec![]),
        concrete_method(u64::MAX, "current", vec![]),
        invalid,
        false_inherent,
    ] {
        let call = request(target, vec![("argument", argument(&counter))]);
        assert!(prepare_call(&call.encode_to_vec()).is_err());
        assert!(HANDLE_TABLE.resolve(counter.key).is_some());
    }
    assert!(HANDLE_TABLE.release(counter.key));
    assert!(HANDLE_TABLE.release(view.key));
    assert!(HANDLE_TABLE.resolve(counter.key).is_none());
}

#[tokio::test]
async fn inherent_method_wire_call_preserves_state_and_prepared_ownership() {
    use bridge_ctypes::baml_bridge::cffi::concrete_method_target::Dispatch;
    let runtime = runtime();
    let counter = make(&runtime, "make_concrete", BamlHandleType::ConcreteObject).await;
    let target = |member: &str| {
        CallTarget::ConcreteMethod(ConcreteMethodTarget {
            receiver: counter.key,
            class_name: "user.StoredCounter".into(),
            dispatch: Some(Dispatch::Inherent(true)),
            member: member.into(),
            type_args: vec![],
        })
    };
    integer(
        invoke(&runtime, request(target("reset"), vec![("count", int(7))])).await,
        7,
    );
    // An inherent selector cannot invoke an interface body's function by name.
    type_error(invoke(&runtime, request(target("current"), vec![])).await);
    integer(
        invoke(
            &runtime,
            request(concrete_method(counter.key, "current", vec![]), vec![]),
        )
        .await,
        7,
    );
    let prepared = prepare_call(&request(target("reset"), vec![]).encode_to_vec()).unwrap();
    assert!(HANDLE_TABLE.release(counter.key));
    integer(
        BamlOutboundResult::decode(invoke_prepared(runtime, prepared).await.as_slice()).unwrap(),
        0,
    );
}

#[tokio::test]
async fn concrete_method_preparation_pins_method_type_evidence_and_live_arguments() {
    let runtime = runtime();
    let counter = make(&runtime, "make_concrete", BamlHandleType::ConcreteObject).await;
    let view = make(&runtime, "make", BamlHandleType::AdtInterface).await;
    let reflected = make(&runtime, "counter_type", BamlHandleType::AdtType).await;
    let call = request(
        concrete_method(
            counter.key,
            "echo",
            vec![BamlTyArg {
                type_reference: Some(reflected.key),
                ..Default::default()
            }],
        ),
        vec![("value", argument(&view))],
    );
    let prepared = prepare_call(&call.encode_to_vec()).unwrap();
    for key in [counter.key, view.key, reflected.key] {
        assert!(HANDLE_TABLE.release(key));
    }
    let returned = handle(
        ok(
            BamlOutboundResult::decode(invoke_prepared(runtime.clone(), prepared).await.as_slice())
                .unwrap(),
        ),
        BamlHandleType::AdtInterface,
    );
    integer(
        invoke(
            &runtime,
            request(method(returned.key, "current", vec![]), vec![]),
        )
        .await,
        0,
    );
    assert!(HANDLE_TABLE.release(returned.key));
    runtime.shutdown().await;
}
