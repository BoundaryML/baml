use baml_type::{Name, ParamTy, RuntimeTy, TypeName};
use bex_project::{Bex, FsPath};
use bridge_cffi::host_registration::*;
use bridge_ctypes::{HANDLE_TABLE, TransferSession, baml_bridge::cffi::*, runtime_ty_to_proto_ty};
use prost::Message;
use std::{collections::HashMap, sync::Arc};
use sys_native::SysOpsExt;

fn runtime() -> Arc<dyn Bex> {
    bex_project::new(
        vfs::MemoryFS::new().into(),
        bex_project::SysOps::native(),
        HashMap::from([(
            FsPath::from_str("main.baml".into()),
            r#"interface Marker {}
interface Echo {
    type Item
    function echo(self, value: Self.Item) -> Self.Item throws never
    function label(self) -> string throws never { "default" }
}"#
            .into(),
        )]),
    )
    .unwrap()
}
fn template(index: u32) -> BamlTy {
    runtime_ty_to_proto_ty(&RuntimeTy::Interface(
        TypeName::from_dotted_path("user.Echo"),
        vec![],
        vec![(
            Name::new("Item"),
            RuntimeTy::TypeVar(ParamTy::new(index, Name::new("slot")), Default::default()),
        )],
        Default::default(),
    ))
}
fn request(index: u32, args: Vec<BamlTyArg>) -> RegisterHostAdapterRequest {
    RegisterHostAdapterRequest {
        name: "NativeEcho".into(),
        implementations: vec![HostAdapterImplementation {
            interface_template: Some(template(index)),
            methods: vec!["echo".into()],
        }],
        type_args: args,
    }
}
fn string_arg() -> BamlTyArg {
    BamlTyArg {
        type_value: Some(runtime_ty_to_proto_ty(&RuntimeTy::string())),
        ..Default::default()
    }
}
fn keys(registration: &RegisteredHostAdapter) -> Vec<u64> {
    let mut keys = vec![
        registration.adapter_type.as_ref().unwrap().key,
        registration.class_type.as_ref().unwrap().key,
    ];
    keys.extend(registration.interface_types.iter().map(|ty| ty.key));
    keys
}
async fn register(
    runtime: Arc<dyn Bex>,
    session: &TransferSession<'static>,
    request: RegisterHostAdapterRequest,
) -> RegisteredHostAdapter {
    let request = HostOperationRequest {
        operation: Some(host_operation_request::Operation::Register(request)),
    };
    let prepared = prepare_operation(&request.encode_to_vec()).unwrap();
    let (receipt, output) = session
        .stage(execute_operation(runtime, prepared).await)
        .unwrap()
        .handoff();
    let Some(host_operation_result::Result::Registered(output)) =
        HostOperationResult::decode(output.as_slice())
            .unwrap()
            .result
    else {
        panic!("expected successful registration");
    };
    session.adopt(receipt, &keys(&output)).unwrap();
    output
}
fn host(key: u64, kind: BamlHandleType) -> InboundValue {
    InboundValue {
        value: Some(inbound_value::Value::Handle(BamlHandle {
            key,
            handle_type: kind as i32,
        })),
        ..Default::default()
    }
}

#[tokio::test]
async fn administrative_failures_use_the_shared_error_envelope() {
    let runtime = runtime();
    let session = TransferSession::new(&HANDLE_TABLE);
    let mut request = request(1, vec![string_arg()]);
    request.implementations[0].methods.clear();
    let prepared = prepare_operation(
        &HostOperationRequest {
            operation: Some(host_operation_request::Operation::Register(request)),
        }
        .encode_to_vec(),
    )
    .unwrap();
    let pending = session
        .stage(execute_operation(runtime, prepared).await)
        .unwrap();
    let result = HostOperationResult::decode(pending.payload().as_slice()).unwrap();
    let Some(host_operation_result::Result::Failure(failure)) = result.result else {
        panic!("missing required host method must reject registration");
    };
    assert!(matches!(
        failure.result,
        Some(baml_outbound_result::Result::Error(_))
    ));
    drop(pending);
    assert_eq!(session.pending_count(), 0);

    let error = prepare_operation(&[])
        .err()
        .expect("empty operation must reject");
    let pending = session.stage(operation_error(error)).unwrap();
    let result = HostOperationResult::decode(pending.payload().as_slice()).unwrap();
    assert!(matches!(
        result.result,
        Some(host_operation_result::Result::Failure(_))
    ));
    drop(pending);
    assert_eq!(session.pending_count(), 0);
}
fn create(registration: &RegisteredHostAdapter) -> CreateHostAdapterRequest {
    CreateHostAdapterRequest {
        adapter_type: registration.adapter_type.as_ref().unwrap().key,
        receiver: Some(host(801, BamlHandleType::HostValueOpaque)),
        callbacks: vec![host(802, BamlHandleType::HostValueCallable)],
    }
}
fn handle(value: BamlOutboundValue) -> BamlOutboundHandle {
    let Some(baml_outbound_value::Value::HandleValue(handle)) = value.value else {
        panic!("expected handle")
    };
    handle
}
fn adopt_value(
    session: &TransferSession<'static>,
    encoded: bridge_ctypes::EncodedTransfer<'static, BamlOutboundValue>,
) -> BamlOutboundHandle {
    let (receipt, output) = session.stage(encoded).unwrap().handoff();
    let handle = handle(output);
    session.adopt(receipt, &[handle.key]).unwrap();
    handle
}
extern "C" fn dispatch(key: u64, id: u32, bytes: *const u8, len: usize) {
    // The native dispatcher keeps the wire buffer alive for this invocation.
    let call = BamlToHostCall::decode(unsafe { std::slice::from_raw_parts(bytes, len) }).ok();
    let value = call
        .and_then(|call| call.args.into_iter().next())
        .and_then(|arg| arg.value)
        .and_then(|value| value.value);
    let returned = match (key, value) {
        (802, Some(baml_outbound_value::Value::StringValue(value))) => {
            bex_project::BexExternalValue::String(value.into())
        }
        _ => bex_project::BexExternalValue::Null,
    };
    sys_native::host_dispatch::complete_with_value(id, returned);
}

#[tokio::test]
async fn encoded_registration_instance_projection_and_callback_round_trip() {
    sys_native::host_dispatch::set_dispatch_fn(dispatch);
    let runtime = runtime();
    let session = TransferSession::new(&HANDLE_TABLE);
    let registration = register(runtime.clone(), &session, request(1, vec![string_arg()])).await;
    assert_eq!(
        registration.adapter_type.as_ref().unwrap().handle_type,
        BamlHandleType::HostAdapterType as i32
    );
    let entry = HANDLE_TABLE
        .resolve(registration.adapter_type.as_ref().unwrap().key)
        .unwrap();
    assert!(bex_project::BexExternalValue::try_from((*entry).clone()).is_err());
    drop(entry);
    let prepared = prepare_instance(&create(&registration).encode_to_vec()).unwrap();
    // Synchronous preparation retains the type even after its SDK lease closes.
    assert!(HANDLE_TABLE.release(registration.adapter_type.as_ref().unwrap().key));
    let instance = adopt_value(
        &session,
        create_instance_encoded(runtime.clone(), prepared)
            .await
            .unwrap(),
    );
    let projection = prepare_projection(
        &ProjectInterfaceRequest {
            receiver: instance.key,
            interface_type: Some(BamlTyArg {
                type_reference: Some(registration.interface_types[0].key),
                ..Default::default()
            }),
        }
        .encode_to_vec(),
    )
    .unwrap();
    assert!(HANDLE_TABLE.release(instance.key));
    assert!(HANDLE_TABLE.release(registration.class_type.as_ref().unwrap().key));
    assert!(HANDLE_TABLE.release(registration.interface_types[0].key));
    let view = adopt_value(
        &session,
        project_encoded(runtime.clone(), projection).await.unwrap(),
    );
    let call = CallFunctionArgs {
        call_id: sys_types::CallId::next().0,
        call_target: Some(call_function_args::CallTarget::InterfaceMethod(
            InterfaceMethodTarget {
                view: view.key,
                member: "echo".into(),
                type_args: vec![],
            },
        )),
        kwargs: vec![InboundMapEntry {
            key: Some(inbound_map_entry::Key::StringKey("value".into())),
            value: Some(InboundValue {
                value: Some(inbound_value::Value::StringValue("Ada".into())),
                ..Default::default()
            }),
        }],
        type_args: vec![],
    };
    let prepared = bridge_cffi::prepare_call(&call.encode_to_vec()).unwrap();
    assert!(HANDLE_TABLE.release(view.key));
    let output = bridge_cffi::invoke_prepared(runtime, prepared).await;
    let result = BamlOutboundResult::decode(output.as_slice()).unwrap();
    let Some(baml_outbound_result::Result::Ok(value)) = result.result else {
        panic!("{result:?}")
    };
    assert_eq!(
        value.value,
        Some(baml_outbound_value::Value::StringValue("Ada".into()))
    );
    assert_eq!(session.pending_count(), 0);
}
#[tokio::test]
async fn unadopted_registration_rolls_back_all_output_handles() {
    let runtime = runtime();
    let session = TransferSession::new(&HANDLE_TABLE);
    for _ in 0..3 {
        let prepared =
            prepare_registration(&request(1, vec![string_arg()]).encode_to_vec()).unwrap();
        let delivery = session
            .stage(register_encoded(runtime.clone(), prepared).await.unwrap())
            .unwrap();
        let keys = keys(delivery.payload());
        assert!(keys.iter().all(|key| HANDLE_TABLE.resolve(*key).is_some()));
        drop(delivery);
        assert!(keys.iter().all(|key| HANDLE_TABLE.resolve(*key).is_none()));
        assert_eq!(session.pending_count(), 0);
    }
    let prepared = prepare_registration(&request(1, vec![string_arg()]).encode_to_vec()).unwrap();
    let (receipt, output) = session
        .stage(register_encoded(runtime, prepared).await.unwrap())
        .unwrap()
        .handoff();
    let key = output.adapter_type.as_ref().unwrap().key;
    session.adopt(receipt, &[key]).unwrap();
    assert!(
        HANDLE_TABLE
            .resolve(output.class_type.as_ref().unwrap().key)
            .is_none()
    );
    assert!(
        HANDLE_TABLE
            .resolve(output.interface_types[0].key)
            .is_none()
    );
    assert!(HANDLE_TABLE.release(key));
    assert_eq!(session.pending_count(), 0);
}
static RELEASED: std::sync::Mutex<Vec<u64>> = std::sync::Mutex::new(Vec::new());
extern "C" fn release(key: u64) {
    RELEASED.lock().unwrap().push(key);
}
#[tokio::test]
async fn rejected_or_abandoned_preparation_releases_all_host_inputs() {
    bex_project::host_release_dispatch::install(release).unwrap();
    let runtime = runtime();
    let session = TransferSession::new(&HANDLE_TABLE);
    let registration = register(runtime.clone(), &session, request(1, vec![string_arg()])).await;
    let prepared = prepare_instance(&create(&registration).encode_to_vec()).unwrap();
    drop(prepared);
    let mut released = RELEASED.lock().unwrap().clone();
    released.sort();
    assert_eq!(released, vec![801, 802]);
    RELEASED.lock().unwrap().clear();
    let invalid = CreateHostAdapterRequest {
        adapter_type: u64::MAX,
        receiver: Some(host(803, BamlHandleType::HostValueOpaque)),
        callbacks: vec![host(804, BamlHandleType::HostValueCallable)],
    };
    assert!(prepare_instance(&invalid.encode_to_vec()).is_err());
    let mut released = RELEASED.lock().unwrap().clone();
    released.sort();
    assert_eq!(released, vec![803, 804]);
    RELEASED.lock().unwrap().clear();
    let request = CreateHostAdapterRequest {
        adapter_type: registration.adapter_type.as_ref().unwrap().key,
        receiver: Some(host(805, BamlHandleType::HostValueOpaque)),
        callbacks: vec![host(806, BamlHandleType::HostValueCallable)],
    };
    let prepared = prepare_instance(&request.encode_to_vec()).unwrap();
    drop(create_instance_encoded(runtime.clone(), prepared));
    let mut released = RELEASED.lock().unwrap().clone();
    released.sort();
    assert_eq!(released, vec![805, 806]);
    RELEASED.lock().unwrap().clear();
    let request = CreateHostAdapterRequest {
        adapter_type: registration.adapter_type.as_ref().unwrap().key,
        receiver: Some(host(807, BamlHandleType::HostValueOpaque)),
        callbacks: vec![
            host(808, BamlHandleType::HostValueCallable),
            host(809, BamlHandleType::HostValueCallable),
        ],
    };
    let prepared = prepare_instance(&request.encode_to_vec()).unwrap();
    assert!(create_instance_encoded(runtime, prepared).await.is_err());
    let mut released = RELEASED.lock().unwrap().clone();
    released.sort();
    assert_eq!(released, vec![807, 808, 809]);
    for key in keys(&registration) {
        assert!(HANDLE_TABLE.release(key));
    }
}
#[tokio::test]
async fn registration_pins_live_type_bindings_before_execution_and_rejects_foreign_types() {
    let first = runtime();
    let second = runtime();
    let session = TransferSession::new(&HANDLE_TABLE);
    // Self is a real slot even when no method argument supplies evidence for it.
    let original = register(first.clone(), &session, request(0, vec![])).await;
    let req = request(
        1,
        vec![BamlTyArg {
            type_reference: Some(original.class_type.as_ref().unwrap().key),
            ..Default::default()
        }],
    );
    let valid = prepare_registration(&req.encode_to_vec()).unwrap();
    let foreign = prepare_registration(&req.encode_to_vec()).unwrap();
    let whole_interface = RegisterHostAdapterRequest {
        name: "RetainedInterface".into(),
        implementations: vec![HostAdapterImplementation {
            interface_template: Some(runtime_ty_to_proto_ty(&RuntimeTy::TypeVar(
                ParamTy::new(1, Name::new("I")),
                Default::default(),
            ))),
            methods: vec!["echo".into()],
        }],
        type_args: vec![BamlTyArg {
            type_reference: Some(original.interface_types[0].key),
            ..Default::default()
        }],
    };
    let whole_interface = prepare_registration(&whole_interface.encode_to_vec()).unwrap();
    for key in keys(&original) {
        assert!(HANDLE_TABLE.release(key));
    }
    assert!(register_encoded(second, foreign).await.is_err());
    let delivery = session
        .stage(register_encoded(first.clone(), valid).await.unwrap())
        .unwrap();
    assert_eq!(delivery.payload().callbacks.len(), 1);
    drop(delivery);
    let delivery = session
        .stage(register_encoded(first, whole_interface).await.unwrap())
        .unwrap();
    assert_eq!(delivery.payload().callbacks.len(), 1);
    drop(delivery);
    assert_eq!(session.pending_count(), 0);
}
