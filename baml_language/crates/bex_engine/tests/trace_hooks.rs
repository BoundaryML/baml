//! Host entry, moving GC and cancellation are observable only from the embedder.
mod common;

use std::{sync::Arc, time::Duration};

use bex_engine::{
    BexEngine, BexExternalValue, CancellationToken, EngineError, FunctionCallContextBuilder,
};
use bex_heap::CollectionLevel;
use btel_types::{
    InvocationMode,
    context::{ContextPatch, ContextValue},
};
use sys_native::SysOpsExt;

const SOURCE: &str = r#"
class Box<T> { value T calls int release bool }
function make_box() -> Box<string> { Box { value: "survives collection", calls: 0, release: false } }
function read_count(box: Box<string>) -> int { box.calls }
function release(box: Box<string>) -> void { box.release = true; }
function select<T>(box: Box<T>, settings: trace.Settings) -> trace.Options? throws never {
    box.calls += 1;
    assert.equal(settings.context.metadata["entry"], "caller");
    assert.equal(trace.current_context().metadata.length(), 0);
    while (!box.release) {
        baml.sys.sleep(baml.time.Duration.from_milliseconds(1)) catch (error) { baml.errors.Io => {} };
    }
    assert.equal(box.calls, 1);
    trace.span(inputs = true).context(metadata = { "decision": "hook" })
}
/// baml:$trace=select
function target<T>(box: Box<T>) -> T {
    assert.equal(box.calls, 1);
    assert.equal(trace.current_context().metadata["entry"], "caller");
    assert.equal(trace.current_context().metadata["decision"], "hook");
    box.value
}
function slow() -> trace.Options? throws never {
    baml.sys.sleep(baml.time.Duration.from_milliseconds(60000)) catch (error) { baml.errors.Io => {} };
    null
}
/// baml:$trace=slow
function cancel_target() -> int { baml.sys.panic("cancelled hook must not run target") }
function recovery_hook() -> trace.Options throws never { trace.hidden().context(metadata = { "hook": true }) }
/// baml:$trace=recovery_hook
function recovery() -> int { assert.equal(trace.current_context().metadata["hook"], true); 42 }
"#;

fn engine() -> Arc<BexEngine> {
    Arc::new(
        BexEngine::new(
            common::compile_for_engine(SOURCE),
            Arc::new(sys_native::SysOps::native()),
            vec![],
        )
        .unwrap(),
    )
}

#[tokio::test]
async fn hook_and_target_call_paths_share_the_authored_caller() {
    use std::collections::HashMap;

    use bex_engine::TelemetryRecording;
    use btel_recorder::{RecordingConfig, proto};

    let root = tempfile::tempdir().unwrap();
    let engine = Arc::new(
        BexEngine::new_with_telemetry_recording(
            common::compile_for_engine(
                r#"
function select() -> trace.Options throws never { trace.rich() }
/// baml:$trace=select
function target() -> int { 42 }
function main() -> int { target() }
"#,
            ),
            Arc::new(sys_native::SysOps::native()),
            vec![],
            None,
            btel_clock::ClockMode::Monotonic,
            TelemetryRecording::local_files_in(root.path(), RecordingConfig::default()),
        )
        .unwrap(),
    );
    for entry in ["target", "main"] {
        assert_eq!(
            engine
                .call_function(
                    entry,
                    vec![],
                    FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
                    true,
                )
                .await
                .unwrap(),
            BexExternalValue::Int(42)
        );
    }
    engine.shutdown().await;
    let files = btel_file::read_directory(engine.telemetry_recording_directory().unwrap()).unwrap();
    assert!(files.issues.is_empty(), "{:?}", files.issues);
    let definitions: Vec<_> = files
        .files
        .iter()
        .filter_map(|file| file.definitions.as_ref())
        .collect();
    let names: HashMap<_, _> = definitions
        .iter()
        .flat_map(|defs| &defs.functions)
        .filter_map(|function| match &function.resolution {
            Some(proto::function_definition::Resolution::Metadata(metadata)) => {
                Some((function.function_id, metadata.fqn.as_str()))
            }
            _ => None,
        })
        .collect();
    let main_id = *names
        .iter()
        .find(|(_, name)| **name == "user.main")
        .unwrap()
        .0;
    let main_path = definitions
        .iter()
        .flat_map(|defs| &defs.call_paths)
        .find(|path| path.callee_function_id == main_id)
        .unwrap();
    for name in ["user.select", "user.target"] {
        let paths: Vec<_> = definitions
            .iter()
            .flat_map(|defs| &defs.call_paths)
            .filter(|path| names.get(&path.callee_function_id) == Some(&name))
            .collect();
        assert_eq!(paths.len(), 2, "{name}: {paths:?}");
        let root_path = paths
            .iter()
            .find(|path| path.parent_call_path_id == 0)
            .unwrap();
        assert_eq!(root_path.visible_caller_function_id, None, "{name}");
        let called_path = paths
            .iter()
            .find(|path| path.parent_call_path_id != 0)
            .unwrap();
        assert_eq!(
            called_path.parent_call_path_id, main_path.call_path_id,
            "{name}"
        );
        assert_eq!(
            called_path.visible_caller_function_id,
            Some(main_id),
            "{name}"
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn sdk_generic_hook_arguments_and_continuation_survive_moving_gc() {
    let engine = engine();
    let context = FunctionCallContextBuilder::new(sys_types::CallId::next())
        .with_trace_options(bex_vm_types::trace::TraceOptionsData {
            mode: Some(InvocationMode::Span),
            context: Some(Arc::new(ContextPatch {
                metadata: [("entry".into(), Some(ContextValue::String("caller".into())))].into(),
                ..Default::default()
            })),
            ..Default::default()
        })
        .build();
    let input = engine
        .call_function(
            "make_box",
            vec![],
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            false,
        )
        .await
        .unwrap();
    assert!(
        matches!(input, BexExternalValue::Handle(_)),
        "use one shared live object across invocations"
    );
    let task = tokio::spawn({
        let engine = engine.clone();
        let input = input.clone();
        async move {
            engine
                .call_function("target", vec![input], context, true)
                .await
        }
    });
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let calls = engine
                .call_function(
                    "read_count",
                    vec![input.clone()],
                    FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
                    true,
                )
                .await
                .unwrap();
            if calls == BexExternalValue::Int(1) {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("hook must be running before GC");
    assert!(!task.is_finished(), "hook is gated until after collection");
    let stats = engine.collect_garbage(CollectionLevel::Major).await;
    assert!(stats.live_count > 0);
    engine
        .call_function(
            "release",
            vec![input],
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            true,
        )
        .await
        .unwrap();
    let value = tokio::time::timeout(Duration::from_secs(5), task)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(
        value,
        BexExternalValue::String("survives collection".into())
    );
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cancelling_a_hook_propagates_and_does_not_suppress_later_sdk_calls() {
    let engine = engine();
    let cancel = CancellationToken::new();
    let task = tokio::spawn({
        let engine = engine.clone();
        let cancel = cancel.clone();
        async move {
            engine
                .call_function(
                    "cancel_target",
                    vec![],
                    FunctionCallContextBuilder::new(sys_types::CallId::next())
                        .with_cancel_token(cancel)
                        .build(),
                    true,
                )
                .await
        }
    });
    tokio::time::sleep(Duration::from_millis(40)).await;
    cancel.cancel();
    let result = tokio::time::timeout(Duration::from_secs(5), task)
        .await
        .unwrap()
        .unwrap();
    let Err(EngineError::UnhandledThrow { value, trace, .. }) = result else {
        panic!("cancellation must propagate: {result:?}")
    };
    let BexExternalValue::Instance { class_name, .. } = value.as_ref() else {
        panic!("expected cancellation panic")
    };
    assert_eq!(class_name, bex_engine::CANCELLED_PANIC_CLASS);
    assert!(
        !format!("{trace:?}").contains("cancel_target"),
        "pending target must not appear as executing"
    );
    assert_eq!(
        engine
            .call_function(
                "recovery",
                vec![],
                FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
                true
            )
            .await
            .unwrap(),
        BexExternalValue::Int(42)
    );
    engine.shutdown().await;
}

#[tokio::test]
async fn linked_hooked_callee_cannot_enter_through_call_exact_args() {
    // A call emitted against an external package has no source hook metadata.
    // Reproduce that call site before the engine classifies the linked image.
    let mut program = common::compile_for_engine(
        r#"
        function hook() -> trace.Options throws never { trace.context(metadata = { "executed": true }) }
        /// baml:$trace=hook
        function target() -> int { assert.equal(trace.current_context().metadata["executed"], true); 42 }
        function main() -> int { target() }
    "#,
    );
    let mut rewritten = 0;
    for object in &mut program.objects.0 {
        if let bex_vm_types::Object::Function(function) = object {
            for instruction in &mut function.bytecode.instructions {
                if let bex_vm_types::Instruction::CallHooked { callee, ntypeargs } = *instruction {
                    *instruction = bex_vm_types::Instruction::Call { callee, ntypeargs };
                    rewritten += 1;
                }
            }
            function.bytecode.compact = Some(function.bytecode.lower_to_compact());
        }
    }
    assert_eq!(rewritten, 1);
    let engine =
        Arc::new(BexEngine::new(program, Arc::new(sys_native::SysOps::native()), vec![]).unwrap());
    assert_eq!(
        engine
            .call_function(
                "main",
                vec![],
                FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
                true
            )
            .await
            .unwrap(),
        BexExternalValue::Int(42)
    );
    engine.shutdown().await;
}

#[tokio::test]
async fn internal_vm_failure_in_a_hook_is_not_treated_as_a_null_decision() {
    // The compiler never emits a non-callable indirect callee. Deliberately
    // break that invariant to test the host-visible internal-error boundary.
    let mut program = common::compile_for_engine(SOURCE);
    let mut replaced = false;
    for object in &mut program.objects.0 {
        if let bex_vm_types::Object::Function(function) = object {
            if function.name == "user.recovery_hook" {
                function.bytecode = bex_vm_types::bytecode::Bytecode {
                    instructions: vec![
                        bex_vm_types::Instruction::LoadConst(0),
                        bex_vm_types::Instruction::CallIndirect,
                        bex_vm_types::Instruction::Return,
                    ],
                    constants: vec![bex_vm_types::ConstValue::Int(1)],
                    ..Default::default()
                };
                function.bytecode.compact = Some(function.bytecode.lower_to_compact());
                replaced = true;
            }
        }
    }
    assert!(replaced);
    let engine =
        Arc::new(BexEngine::new(program, Arc::new(sys_native::SysOps::native()), vec![]).unwrap());
    let result = engine
        .call_function(
            "recovery",
            vec![],
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            true,
        )
        .await;
    assert!(
        matches!(
            result,
            Err(EngineError::VmInternalError(_) | EngineError::TracedVmInternalError { .. })
        ),
        "fatal VM errors must escape: {result:?}"
    );
    engine.shutdown().await;
}

#[tokio::test]
async fn telemetry_off_selects_ordinary_bytecode_and_never_executes_hooks() {
    use btel_settings::artifact::{ArtifactTelemetry, RecordingLevel};
    let program = common::compile_for_engine(include_str!(
        "../../baml_cli/tests/fixtures/trace_disabled/hooks.baml"
    ));
    let main_index = program.rendered_callables()["user.off_main"].object.raw();
    let target_index = program.rendered_callables()["user.off_target"].object.raw();
    let builtin_index = program.rendered_callables()["user.off_builtin"]
        .object
        .raw();
    let engine = Arc::new(
        BexEngine::new_with_config(
            program,
            Arc::new(sys_native::SysOps::native()),
            vec![],
            bex_engine::EngineConfig {
                artifact_telemetry: Some(ArtifactTelemetry {
                    recording_level: RecordingLevel::Off,
                    ..Default::default()
                }),
                ..Default::default()
            },
        )
        .unwrap(),
    );
    for index in [main_index, target_index, builtin_index] {
        let pointer = engine.heap().compile_time_ptr(index);
        // SAFETY: this immutable compile-time object is owned by the engine
        // for the full test and never moves during garbage collection.
        #[allow(unsafe_code)]
        let bex_vm_types::Object::Function(function) = (unsafe { pointer.get() }) else {
            panic!("function")
        };
        let compact = function.bytecode.compact.as_ref().unwrap();
        assert!(compact.trace_hook_finish_pc.is_none());
        let mut pc = 0;
        while pc < compact.code.len() {
            let opcode = bex_vm_types::bytecode::OpCode::try_from(compact.code[pc]).unwrap();
            assert!(!matches!(
                opcode,
                bex_vm_types::bytecode::OpCode::CallHooked
                    | bex_vm_types::bytecode::OpCode::BeginTraceHook
                    | bex_vm_types::bytecode::OpCode::EndTraceHook
                    | bex_vm_types::bytecode::OpCode::TraceHookHidden
                    | bex_vm_types::bytecode::OpCode::TraceHookTiming
                    | bex_vm_types::bytecode::OpCode::TraceHookSpan
                    | bex_vm_types::bytecode::OpCode::TraceHookRich
            ));
            pc += opcode.encoded_size();
        }
    }
    assert_eq!(
        engine
            .call_function(
                "off_main",
                vec![],
                FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
                true
            )
            .await
            .unwrap(),
        BexExternalValue::Int(42)
    );
    let counts = BexExternalValue::instance(
        "OffCounts",
        [
            ("hooks", BexExternalValue::Int(0)),
            ("defaults", BexExternalValue::Int(0)),
            ("bodies", BexExternalValue::Int(0)),
        ]
        .into(),
    );
    assert_eq!(
        engine
            .call_function_bound_args(
                "off_target",
                vec![
                    bex_engine::BexCallArg::Provided(Box::new(counts)),
                    bex_engine::BexCallArg::OmittedDefault,
                ],
                FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
                true
            )
            .await
            .unwrap(),
        BexExternalValue::Int(17)
    );
    engine.shutdown().await;
}

#[tokio::test]
async fn builtin_mode_hooks_use_single_opcodes_and_user_names_still_call() {
    use bex_vm_types::Instruction;
    let program = common::compile_for_engine(
        r#"
/// baml:$trace=trace.hidden
function hidden_target() -> bool { trace.current_span_id() != null }
/// baml:$trace=trace.timing
function timing_target() -> bool { trace.current_span_id() != null }
/// baml:$trace=trace.span
function span_target() -> bool { trace.current_span_id() != null }
/// baml:$trace=trace.rich
function rich_target() -> bool { trace.current_span_id() != null }
/// baml:$trace=trace.hidden
function reserved_target(expected: trace.SpanId) -> bool { trace.current_span_id() == expected }
function hidden() -> trace.Options throws never {
    assert.equal(hidden_target($trace = trace.rich()), true);
    trace.context(metadata = { "user_hook": true })
}
/// baml:$trace=hidden
function user_target() -> bool { trace.current_context().metadata["user_hook"] == true }
function main() -> int {
    assert.equal(hidden_target($trace = trace.rich()), false);
    assert.equal(timing_target($trace = trace.rich()), false);
    assert.equal(span_target($trace = trace.hidden()), true);
    assert.equal(rich_target($trace = trace.hidden()), true);
    let reserved = trace.rich().reserve();
    assert.equal(reserved_target(reserved.id(), $trace = reserved), true);
    assert.equal(user_target(), true);
    42
}
"#,
    );
    for (name, instruction) in [
        ("user.hidden_target", Instruction::TraceHookHidden),
        ("user.timing_target", Instruction::TraceHookTiming),
        ("user.span_target", Instruction::TraceHookSpan),
        ("user.rich_target", Instruction::TraceHookRich),
    ] {
        let index = program.rendered_callables()[name].object;
        let bex_vm_types::Object::Function(function) = &program.objects[index] else {
            panic!("function")
        };
        assert_eq!(
            function
                .bytecode
                .instructions
                .iter()
                .filter(|op| **op == instruction)
                .count(),
            1,
            "{name}"
        );
        assert!(
            !function.bytecode.instructions.iter().any(|op| matches!(
                op,
                Instruction::BeginTraceHook(_) | Instruction::EndTraceHook
            )),
            "{name}"
        );
    }
    let engine =
        Arc::new(BexEngine::new(program, Arc::new(sys_native::SysOps::native()), vec![]).unwrap());
    assert_eq!(
        engine
            .call_function(
                "main",
                vec![],
                FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
                true
            )
            .await
            .unwrap(),
        BexExternalValue::Int(42)
    );
    engine.shutdown().await;
}
