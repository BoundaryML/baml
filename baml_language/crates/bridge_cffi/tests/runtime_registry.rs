//! These test bridge registration/ownership and ABI transport, which BAML cannot observe.
use std::{
    collections::HashMap,
    sync::{Arc, Barrier},
};

use bex_project::{BexArgs, BexExternalValue};
use bridge_cffi::{get_runtime_by_key, runtime_key};

#[test]
fn dynamic_instances_are_independent_and_removal_does_not_invalidate_acquired_calls() {
    let files = |value| {
        HashMap::from([(
            "main.baml".into(),
            format!("function value() -> int {{ {value} }}"),
        )])
    };
    let a = bridge_cffi::initialize_runtime(".", files(11)).unwrap();
    let b = bridge_cffi::initialize_runtime(".", files(22)).unwrap();
    let ak = runtime_key(&a).unwrap();
    let bk = runtime_key(&b).unwrap();
    assert_ne!(ak, bk);
    assert!(ak < 1 << 63 && bk < 1 << 63);
    let acquired = get_runtime_by_key(ak).unwrap();
    bridge_cffi::unregister_runtime(ak).unwrap();
    assert!(get_runtime_by_key(ak).is_err());
    let tokio = bridge_cffi::get_tokio_runtime().unwrap();
    for (runtime, expected) in [(acquired, 11), (b, 22)] {
        let ctx = bridge_cffi::function_call_context_builder(bex_project::CallId(
            bridge_cffi::new_function_call_id(),
        ))
        .build();
        let value = tokio
            .block_on(runtime.call_function(
                "value",
                BexArgs {
                    required: Default::default(),
                    optional: Default::default(),
                },
                ctx,
            ))
            .unwrap();
        assert!(matches!(value, BexExternalValue::Int(n) if n == expected));
    }
    bridge_cffi::unregister_runtime(bk).unwrap();
    let c = bridge_cffi::initialize_runtime(".", files(33)).unwrap();
    assert!(runtime_key(&c).unwrap() > bk);
    bridge_cffi::unregister_runtime(runtime_key(&c).unwrap()).unwrap();
}

#[test]
fn concurrent_generated_registration_is_idempotent_and_conflicts_do_not_replace() {
    bridge_cffi::register_bridge(bridge_cffi::BridgeInfo {
        language: bridge_cffi::BridgeLanguage::Python,
        bridge_runtime_name: "test-bridge".into(),
        bridge_runtime_version: "1.0.0".into(),
        toolchain_version: baml_version::CANONICAL_VERSION.into(),
    })
    .unwrap();
    let bytes = baml_artifact::encode(
        baml_artifact::ArtifactKind::Program,
        &baml_tests::stdlib_prefix::compile_source("function value() -> int { 1 }"),
    )
    .unwrap();
    let key = u64::MAX - 12; // beyond JS safe integers, including the high bit
    let barrier = Arc::new(Barrier::new(8));
    let threads: Vec<_> = (0..8)
        .map(|_| {
            let bytes = bytes.clone();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                barrier.wait();
                bridge_cffi::register_runtime_from_bytecode(key, &bytes, None).unwrap()
            })
        })
        .collect();
    let runtimes: Vec<_> = threads.into_iter().map(|t| t.join().unwrap()).collect();
    for runtime in &runtimes {
        assert!(Arc::ptr_eq(runtime, &runtimes[0]));
    }
    assert_eq!(runtime_key(&runtimes[0]).unwrap(), key);
    let conflicting = baml_tests::stdlib_prefix::compile_source("function value() -> int { 2 }");
    let bytes = baml_artifact::encode(baml_artifact::ArtifactKind::Program, &conflicting).unwrap();
    let error = bridge_cffi::register_runtime_from_bytecode(key, &bytes, None)
        .err()
        .unwrap();
    assert!(error.to_string().contains("Conflicting BAML program"));
    assert!(Arc::ptr_eq(&get_runtime_by_key(key).unwrap(), &runtimes[0]));
    assert!(bridge_cffi::unregister_runtime(key).is_err());
    assert!(bridge_cffi::register_runtime_from_bytecode(1, &bytes, None).is_err());
}

#[test]
fn canonical_identity_excludes_paths_and_artifact_metadata() {
    let mut program = baml_tests::stdlib_prefix::compile_source("function value() -> int { 7 }");
    let encode = |p: &bex_project::Program| {
        baml_artifact::encode(baml_artifact::ArtifactKind::Program, p).unwrap()
    };
    let original = encode(&program);
    let key = baml_program_identity::program_key(&original).unwrap();
    for object in &mut program.objects {
        if let bex_project::Object::Function(function) = object {
            function.source_file = "/another-machine/a/relocated/program.baml".into();
            function.debug_locals.clear();
            function.bytecode.line_table.clear();
            function.bytecode.meta.clear();
        }
    }
    assert_eq!(
        key,
        baml_program_identity::program_key(&encode(&program)).unwrap()
    );
    assert_eq!(
        baml_program_identity::canonical_bytes(&original).unwrap(),
        baml_program_identity::canonical_bytes(&encode(&program)).unwrap()
    );
}

#[test]
fn capabilities_keep_their_origin_and_reject_mixed_runtime_arguments() {
    use bridge_ctypes::{
        CffiHandleTableEntry, HANDLE_TABLE, RuntimeOwner,
        baml_bridge::cffi::{CallFunctionArgs, call_function_args::CallTarget},
    };
    let runtime = || {
        bridge_cffi::initialize_runtime(
            ".",
            HashMap::from([("main.baml".into(), "function value() -> int { 3 }".into())]),
        )
        .unwrap()
    };
    let a = runtime();
    let b = runtime();
    let ak = runtime_key(&a).unwrap();
    let bk = runtime_key(&b).unwrap();
    let handle = HANDLE_TABLE.insert_for_runtime(
        CffiHandleTableEntry::FunctionRef { global_index: 0 },
        Some(RuntimeOwner {
            key: ak,
            runtime: a.clone(),
        }),
    );
    let clone = HANDLE_TABLE.clone_handle(handle).unwrap();
    let args = CallFunctionArgs {
        call_target: Some(CallTarget::FunctionHandle(clone)),
        ..Default::default()
    };
    assert!(Arc::ptr_eq(
        &bridge_cffi::runtime_for_call(None, &args).unwrap(),
        &a
    ));
    assert!(bridge_cffi::runtime_for_call(Some(bk), &args).is_err());
    let foreign = HANDLE_TABLE.insert_for_runtime(
        CffiHandleTableEntry::FunctionRef { global_index: 0 },
        Some(RuntimeOwner {
            key: bk,
            runtime: b.clone(),
        }),
    );
    let mut mixed = args.clone();
    mixed.kwargs.push(bridge_ctypes::baml_bridge::cffi::InboundMapEntry {
        value: Some(bridge_ctypes::baml_bridge::cffi::InboundValue {
            value: Some(bridge_ctypes::baml_bridge::cffi::inbound_value::Value::Handle(
                bridge_ctypes::baml_bridge::cffi::BamlHandle {
                    key: foreign,
                    handle_type: bridge_ctypes::baml_bridge::cffi::BamlHandleType::FunctionRef as i32,
                },
            )),
            ..Default::default()
        }),
        ..Default::default()
    });
    assert!(bridge_cffi::runtime_for_call(None, &mixed).is_err());
    // Host-callable keys belong to another namespace, even when numerically equal.
    if let Some(bridge_ctypes::baml_bridge::cffi::inbound_value::Value::Handle(handle)) = mixed
        .kwargs[0]
        .value
        .as_mut()
        .and_then(|value| value.value.as_mut())
    {
        handle.handle_type =
            bridge_ctypes::baml_bridge::cffi::BamlHandleType::HostValueCallable as i32;
    }
    assert!(bridge_cffi::runtime_for_call(None, &mixed).is_ok());
    HANDLE_TABLE.release(foreign);
    bridge_cffi::unregister_runtime(ak).unwrap();
    assert_eq!(runtime_key(&a).unwrap(), ak);
    assert!(Arc::ptr_eq(
        &bridge_cffi::runtime_for_call(None, &args).unwrap(),
        &a
    ));
    assert!(bridge_cffi::runtime_for_call(Some(ak), &args).is_err());
    assert!(HANDLE_TABLE.release(handle));
    assert!(HANDLE_TABLE.release(clone));
    bridge_cffi::unregister_runtime(bk).unwrap();
}

#[test]
fn cancellation_before_dispatch_is_applied_only_to_the_originating_runtime() {
    use bridge_ctypes::baml_bridge::cffi::{
        BamlOutboundResult, baml_outbound_result::Result as Outcome,
    };
    use prost::Message;

    let make_runtime = || {
        bridge_cffi::initialize_runtime(
            ".",
            HashMap::from([("main.baml".into(), "function value() -> int { 9 }".into())]),
        )
        .unwrap()
    };
    let a = make_runtime();
    let b = make_runtime();
    let cancelled = bridge_cffi::new_function_call_id();
    assert!(bridge_cffi::cancel_function_call_by_id(cancelled));
    let invoke = |runtime, call_id| {
        let context =
            bridge_cffi::function_call_context_builder(bex_project::CallId(call_id)).build();
        let bytes =
            bridge_cffi::get_tokio_runtime()
                .unwrap()
                .block_on(bridge_cffi::call_and_encode(
                    runtime,
                    "value".into(),
                    BexArgs {
                        required: Default::default(),
                        optional: Default::default(),
                    },
                    context,
                ));
        BamlOutboundResult::decode(bytes.as_slice()).unwrap()
    };
    assert!(matches!(
        invoke(a.clone(), cancelled).result,
        Some(Outcome::Panic(_))
    ));
    assert!(matches!(
        invoke(b.clone(), bridge_cffi::new_function_call_id()).result,
        Some(Outcome::Ok(_))
    ));
    assert!(matches!(
        invoke(a.clone(), bridge_cffi::new_function_call_id()).result,
        Some(Outcome::Ok(_))
    ));
    assert!(!bridge_cffi::cancel_function_call_by_id(cancelled));
    bridge_cffi::unregister_runtime(runtime_key(&a).unwrap()).unwrap();
    bridge_cffi::unregister_runtime(runtime_key(&b).unwrap()).unwrap();
}

#[test]
fn abandoned_call_ids_are_released_without_losing_dispatched_cancellation() {
    let abandoned = bridge_cffi::new_function_call_id();
    assert!(bridge_cffi::cancel_function_call_by_id(abandoned));
    bridge_cffi::release_function_call_id(abandoned);
    assert!(!bridge_cffi::cancel_function_call_by_id(abandoned));
    bridge_cffi::release_function_call_id(abandoned);

    let dispatched = bridge_cffi::new_function_call_id();
    let reservation = bridge_cffi::FunctionCallReservation::new(dispatched);
    bridge_cffi::release_function_call_id(dispatched);
    assert!(bridge_cffi::cancel_function_call_by_id(dispatched));
    drop(reservation); // preparation failed before registering an active route
    assert!(!bridge_cffi::cancel_function_call_by_id(dispatched));
    assert!(!bridge_cffi::cancel_function_call_by_id(u64::MAX));
}
