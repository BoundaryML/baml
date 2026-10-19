use std::{
    collections::HashMap,
    sync::{Arc, atomic::AtomicBool},
};

use bex_heap::CollectionLevel;
use bex_vm_types::{FunctionKind, HeapPtr, Object, ObjectIndex, RootHaver, Value};

use crate::errors::VmError;
use crate::{
    telemetry::{TelemetryPolicies, TelemetryState},
    vm::{BexVm, Frame, VmExecState},
};

// Rust tests can install compiled bodies directly.
#[cfg(not(target_arch = "wasm32"))]
#[test]
fn compiled_body_is_indistinguishable_from_bytecode() {
    use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};

    use crate::telemetry::{InvocationOutcome, SpanRecord, TimingRecord};

    static CALLS: AtomicUsize = AtomicUsize::new(0);
    static SAME: AtomicU32 = AtomicU32::new(0);
    fn add_compiled(vm: &mut BexVm, args: &[Value]) -> Result<Value, Box<VmError>> {
        CALLS.fetch_add(1, Ordering::Relaxed);
        vm.add_int(args[0], args[1])
    }
    // `Same<T>` forwards its type argument to the compiled `Add` body.
    fn same_compiled(vm: &mut BexVm, args: &[Value]) -> Result<Value, Box<VmError>> {
        let depth = vm.frame_depth();
        let t = vm.frame_type_arg(depth, 0)?;
        let callee = vm.idx_to_ptr(ObjectIndex::from_raw(SAME.load(Ordering::Relaxed) as usize));
        let _ = vm.call_compiled_with_types(
            callee,
            add_compiled,
            std::slice::from_ref(&t),
            &[args[0], args[1]],
            0,
        )?;
        Ok(Value::bool(vm.value_is_member_of(args[1], &t)))
    }

    const SOURCE: &str = r#"
        function Add(a: int, b: int) -> int { a + b }
        function Same<T>(x: T, y: T) -> bool { Add(1, 2); y is T }
        function AddStr(a: int, b: int) -> string { `${Add(a, b)}|${Same<int>(a, b)}` }
        function Caller(a: int, b: int) -> string {
            AddStr(a, b) catch (e, ctx) {
                baml.panics.IntegerOverflow => {
                    let frames = ctx.stack_trace.frames;
                    frames[frames.length() - 1].function_name
                }
            }
        }
        function Main() -> string {
            `${Caller(1, 2)}|${Caller(baml.Int.max_value(), 1)}`
        }
    "#;

    type Timings = Vec<(btel_types::CallPathId, InvocationOutcome, bool)>;
    type NamedPaths = HashMap<btel_types::CallPathId, (Option<String>, String)>;
    type TypeArgs = Vec<(String, String)>;

    fn named(
        run: &(String, Timings, NamedPaths, TypeArgs),
    ) -> Vec<(Option<String>, String, InvocationOutcome, bool)> {
        run.1
            .iter()
            .map(|(path, outcome, reentry)| {
                let (caller, callee) = run.2[path].clone();
                (caller, callee, *outcome, *reentry)
            })
            .collect()
    }

    fn run(compiled: bool) -> (String, Timings, NamedPaths, TypeArgs) {
        let mut program = baml_test_support::compile_source(SOURCE);
        let entry = program.rendered_callables()["user.Main"].object.raw();
        let add = program.rendered_callables()["user.Add"].object.raw();
        SAME.store(u32::try_from(add).unwrap(), Ordering::Relaxed);
        if compiled {
            for (name, body) in [
                ("user.Add", add_compiled as crate::compiled_rt::InlineBody),
                ("user.Same", same_compiled),
            ] {
                let object = program.rendered_callables()[name].object.raw();
                let Object::Function(function) =
                    &mut program.objects[ObjectIndex::from_raw(object)]
                else {
                    unreachable!()
                };
                assert!(matches!(function.kind, FunctionKind::Bytecode));
                function.compiled_body =
                    Some(test_body(crate::compiled_rt::CompiledFn::Inline(body), 0));
            }
        }
        let same = program.rendered_callables()["user.Same"].object.raw();
        let mut vm = BexVm::from_program(program, Arc::new(AtomicBool::new(false))).unwrap();
        let Object::Function(same) = vm.get_object(vm.heap.compile_time_ptr(same)) else {
            unreachable!()
        };
        vm.telemetry
            .as_ref()
            .unwrap()
            .set_policy(
                same,
                crate::telemetry::TelemetryPolicy {
                    span_from_entry: true,
                    capture_inputs: true,
                    ..crate::telemetry::TelemetryPolicy::NONE
                },
            )
            .unwrap();
        vm.set_entry_point(vm.heap.compile_time_ptr(entry), &[]);
        let result = loop {
            match vm.exec().unwrap() {
                VmExecState::EarlyYield => {}
                VmExecState::Complete(value) => {
                    break vm.as_string(&value).unwrap().to_string();
                }
                other => panic!("unexpected execution state: {other:?}"),
            }
        };
        let telemetry = vm.telemetry.as_ref().unwrap();
        let timings = telemetry
            .timing_records()
            .iter()
            .filter_map(|event| match event {
                TimingRecord::FunctionTimingCompletion {
                    call_path,
                    outcome,
                    reentry,
                    ..
                } => Some((*call_path, *outcome, *reentry)),
                TimingRecord::ThreadSelected { .. } | TimingRecord::SysOpTime { .. } => None,
            })
            .collect();
        let mut named_paths = HashMap::new();
        for event in telemetry.span_records() {
            if let SpanRecord::CallPathDefined {
                call_path,
                visible_caller,
                callee,
                ..
            } = event
            {
                let callee = vm.heap.function_metadata(vm.proof(), *callee).unwrap();
                let caller =
                    visible_caller.map(|id| vm.heap.function_metadata(vm.proof(), id).unwrap().fqn);
                named_paths.insert(*call_path, (caller, callee.fqn));
            }
        }
        let type_args = telemetry
            .span_records()
            .iter()
            .filter_map(|event| match event {
                SpanRecord::FunctionSpanAnnouncement {
                    call_path,
                    captured_type_args: Some(type_args),
                    ..
                } => Some((named_paths[call_path].1.clone(), format!("{type_args:?}"))),
                _ => None,
            })
            .collect();
        (result, timings, named_paths, type_args)
    }

    let bytecode = run(false);
    assert_eq!(CALLS.load(Ordering::Relaxed), 0);
    let compiled = run(true);
    assert_eq!(
        CALLS.load(Ordering::Relaxed),
        3,
        "the compiled bodies must run for every call"
    );

    assert_eq!(bytecode.0, "3|true|user.Add");
    assert_eq!(compiled.0, bytecode.0);

    // Call-path IDs are process-global, so compare the caller and callee names.
    let bytecode_named = named(&bytecode);
    let compiled_named = named(&compiled);
    assert!(bytecode_named.len() >= 9);
    assert_eq!(compiled_named, bytecode_named);
    assert!(
        !bytecode.3.is_empty(),
        "the generic call announces its type arguments"
    );
    assert_eq!(compiled.3, bytecode.3);
    let add_outcomes: Vec<_> = compiled_named
        .iter()
        .filter(|(caller, callee, _, _)| {
            caller.as_deref() == Some("user.AddStr") && callee == "user.Add"
        })
        .map(|(_, _, outcome, _)| *outcome)
        .collect();
    assert_eq!(add_outcomes.len(), 2);
    assert_eq!(add_outcomes[0], InvocationOutcome::Ok);
    assert_ne!(add_outcomes[1], InvocationOutcome::Ok);
}

/// Read an integer from a test value.
pub(crate) fn test_int(value: Value) -> i64 {
    value.as_int().expect("an int")
}

fn test_body(
    body: crate::compiled_rt::CompiledFn,
    slots: u32,
) -> bex_vm_types::types::CompiledBody {
    crate::compiled_rt::CompiledEntry {
        object: 0,
        name: "",
        arity: 0,
        body,
        slots,
        cause_slot: None,
    }
    .erase()
}

/// Run `user.Main` with compiled bodies installed and publish named object indices.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn run_with_compiled_bodies(
    source: &str,
    bodies: &[(&str, crate::compiled_rt::CompiledFn)],
    objects: &[(&str, &std::sync::atomic::AtomicU32)],
    telemetry: bool,
) -> String {
    let level = telemetry.then_some(btel_settings::mode::AutoTelemetryLevel::Medium);
    exec_with_compiled_bodies(source, bodies, objects, level).0
}

#[cfg(not(target_arch = "wasm32"))]
fn exec_with_compiled_bodies(
    source: &str,
    bodies: &[(&str, crate::compiled_rt::CompiledFn)],
    objects: &[(&str, &std::sync::atomic::AtomicU32)],
    level: Option<btel_settings::mode::AutoTelemetryLevel>,
) -> (String, BexVm) {
    let mut program = baml_test_support::compile_source(source);
    let callables = program.rendered_callables();
    let entry = callables["user.Main"].object.raw();
    for (name, slot) in objects {
        let raw = callables.get(*name).map_or_else(
            || {
                program
                    .packages
                    .iter()
                    .find_map(|package| {
                        package.classes.iter().chain(&package.interfaces).find_map(
                            |(path, object)| {
                                // Display names omit the root package.
                                let qualified = std::iter::once(package.name.as_str())
                                    .chain(path.namespace.iter().map(baml_type::Name::as_str))
                                    .chain(std::iter::once(path.name.as_str()))
                                    .collect::<Vec<_>>()
                                    .join(".");
                                (qualified == *name).then(|| object.raw())
                            },
                        )
                    })
                    .unwrap_or_else(|| panic!("no callable, class or interface `{name}`"))
            },
            |callable| callable.object.raw(),
        );
        slot.store(
            u32::try_from(raw).unwrap(),
            std::sync::atomic::Ordering::Relaxed,
        );
    }
    // Interface implementations have display names but no callable map entries.
    let indices: Vec<usize> = bodies
        .iter()
        .map(|(name, _)| {
            callables.get(*name).map_or_else(
                || {
                    (0..program.objects.len())
                        .find(|&index| {
                            program
                                .function_name(ObjectIndex::from_raw(index))
                                .as_deref()
                                == Some(*name)
                        })
                        .unwrap_or_else(|| panic!("no function `{name}`"))
                },
                |callable| callable.object.raw(),
            )
        })
        .collect();
    for ((_, body), index) in bodies.iter().zip(indices) {
        let Object::Function(function) = &mut program.objects[ObjectIndex::from_raw(index)] else {
            unreachable!()
        };
        let slots = match body {
            crate::compiled_rt::CompiledFn::Inline(_) => 0,
            crate::compiled_rt::CompiledFn::Resumable(_) => 3,
        };
        function.compiled_body = Some(test_body(*body, slots));
    }
    let mut vm = BexVm::from_program(program, Arc::new(AtomicBool::new(false))).unwrap();
    vm.telemetry = level.map(|level| {
        TelemetryState::new_root(
            Arc::new(TelemetryPolicies::with_auto_level(level)),
            btel_clock::ClockRuntime::new(btel_clock::ClockMode::Monotonic).start_run(),
            crate::telemetry::test_runtime(),
        )
    });
    vm.set_entry_point(vm.heap.compile_time_ptr(entry), &[]);
    let out = loop {
        match vm.exec() {
            Ok(VmExecState::EarlyYield) => {}
            Ok(VmExecState::Complete(value)) => {
                break vm.as_string(&value).unwrap().to_string();
            }
            Ok(other) => panic!("unexpected execution state: {other:?}"),
            Err(error) => break format!("error: {error:?}"),
        }
    };
    (out, vm)
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn compiled_recursion_hits_the_frame_limit() {
    use std::sync::atomic::{AtomicU32, Ordering};
    static DEEP: AtomicU32 = AtomicU32::new(0);
    fn deep(vm: &mut BexVm, args: &[Value]) -> Result<Value, Box<VmError>> {
        let n = test_int(args[0]);
        if n == 0 {
            return Ok(Value::int(0));
        }
        let me = vm.idx_to_ptr(ObjectIndex::from_raw(DEEP.load(Ordering::Relaxed) as usize));
        let below = vm.call_compiled(me, deep, &[Value::int(n - 1)], 0)?;
        Ok(Value::int(test_int(below) + 1))
    }
    const SOURCE: &str = r#"
        function Deep(n: int) -> int { if (n == 0) { 0 } else { 1 + Deep(n - 1) } }
        function Main() -> string {
            let shallow = Deep(100);
            let d = Deep(300) catch (e) { baml.panics.StackOverflow => -7 };
            let huge = Deep(1000000) catch (e) { baml.panics.StackOverflow => -8 };
            `${shallow}|${d}|${huge}`
        }
    "#;
    for telemetry in [false, true] {
        let out = run_with_compiled_bodies(
            SOURCE,
            &[("user.Deep", crate::compiled_rt::CompiledFn::Inline(deep))],
            &[("user.Deep", &DEEP)],
            telemetry,
        );
        assert_eq!(out, "100|-7|-8", "telemetry: {telemetry}");
    }
}

// Large frames force stack growth before the VM reaches its frame limit.
#[cfg(not(target_arch = "wasm32"))]
#[test]
fn compiled_recursion_grows_the_stack_to_the_frame_limit() {
    use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
    static DEEP: AtomicU32 = AtomicU32::new(0);
    static DEEPEST: AtomicUsize = AtomicUsize::new(0);
    fn deep(vm: &mut BexVm, args: &[Value]) -> Result<Value, Box<VmError>> {
        let n = test_int(args[0]);
        DEEPEST.fetch_max(vm.frame_depth(), Ordering::Relaxed);
        if n == 0 {
            return Ok(Value::int(0));
        }
        let mut pad = [0u64; 8192];
        let slot = usize::try_from(n).unwrap() % pad.len();
        pad[slot] = 1;
        std::hint::black_box(&mut pad);
        let me = vm.idx_to_ptr(ObjectIndex::from_raw(DEEP.load(Ordering::Relaxed) as usize));
        let below = vm.call_compiled(me, deep, &[Value::int(n - 1)], 0)?;
        let one = std::hint::black_box(&pad)[slot];
        Ok(Value::int(test_int(below) + i64::try_from(one).unwrap()))
    }
    const SOURCE: &str = r#"
        function Deep(n: int) -> int { if (n == 0) { 0 } else { 1 + Deep(n - 1) } }
        function Main() -> string {
            let shallow = Deep(100);
            let d = Deep(300) catch (e) { baml.panics.StackOverflow => -7 };
            let huge = Deep(1000000) catch (e) { baml.panics.StackOverflow => -8 };
            `${shallow}|${d}|${huge}`
        }
    "#;
    let run = || {
        for telemetry in [false, true] {
            DEEPEST.store(0, Ordering::Relaxed);
            let out = run_with_compiled_bodies(
                SOURCE,
                &[("user.Deep", crate::compiled_rt::CompiledFn::Inline(deep))],
                &[("user.Deep", &DEEP)],
                telemetry,
            );
            assert_eq!(out, "100|-7|-8", "telemetry: {telemetry}");
            assert_eq!(
                DEEPEST.load(Ordering::Relaxed),
                crate::vm::MAX_FRAMES,
                "telemetry: {telemetry}"
            );
        }
    };
    run();
    std::thread::Builder::new()
        .stack_size(2 << 20)
        .spawn(run)
        .unwrap()
        .join()
        .unwrap();
}

// Type arguments must be bound before an inline body runs, including when it throws.
#[cfg(not(target_arch = "wasm32"))]
#[test]
fn inline_body_of_a_generic_function_sees_the_type_arguments() {
    fn name_of(vm: &mut BexVm, args: &[Value]) -> Result<Value, Box<VmError>> {
        let Some(Frame::Bytecode(frame)) = vm.frames.last() else {
            unreachable!("the body runs in its own frame")
        };
        let text = match frame.type_args.as_slice() {
            [bex_vm_types::RealizedTy::Int] => "int",
            [bex_vm_types::RealizedTy::String] => "string",
            [bex_vm_types::RealizedTy::List(element)]
                if **element == bex_vm_types::RealizedTy::Int =>
            {
                "int[]"
            }
            other => panic!("unexpected frame type arguments {other:?}"),
        };
        let throws = args[0]
            .as_object_ptr()
            .is_some_and(|ptr| matches!(vm.get_object(ptr), Object::String(s) if s == "throw"));
        if throws {
            let thrown = vm.string_value(text);
            return Err(crate::compiled_rt::throw(thrown));
        }
        Ok(vm.string_value(text))
    }
    const SOURCE: &str = r#"
        function NameOf<T>(x: T) -> string { "bytecode" }
        function Main() -> string {
            let caught = NameOf<string>("throw") catch (e) { string => e };
            `${NameOf<int>(1)}|${NameOf<string>("a")}|${NameOf([1])}|${caught}`
        }
    "#;
    for telemetry in [false, true] {
        let out = run_with_compiled_bodies(
            SOURCE,
            &[(
                "user.NameOf",
                crate::compiled_rt::CompiledFn::Inline(name_of),
            )],
            &[],
            telemetry,
        );
        assert_eq!(out, "int|string|int[]|string", "telemetry: {telemetry}");
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn compiled_caller_passes_type_arguments_to_a_compiled_callee() {
    use std::sync::atomic::{AtomicU32, Ordering};
    static NAME_OF: AtomicU32 = AtomicU32::new(0);
    fn frame_type_arg(vm: &BexVm) -> bex_vm_types::RealizedTy {
        let Some(Frame::Bytecode(frame)) = vm.frames.last() else {
            unreachable!("the body runs in its own frame")
        };
        assert_eq!(frame.type_args.len(), 1, "{:?}", frame.type_args);
        frame.type_args[0].clone()
    }
    #[allow(clippy::unnecessary_wraps)]
    fn name_of(vm: &mut BexVm, _args: &[Value]) -> Result<Value, Box<VmError>> {
        let text = match frame_type_arg(vm) {
            bex_vm_types::RealizedTy::Int => "int",
            bex_vm_types::RealizedTy::String => "string",
            other => panic!("unexpected frame type argument {other:?}"),
        };
        Ok(vm.string_value(text))
    }
    fn forward(vm: &mut BexVm, args: &[Value]) -> Result<Value, Box<VmError>> {
        let ty = frame_type_arg(vm);
        let callee = vm.idx_to_ptr(ObjectIndex::from_raw(
            NAME_OF.load(Ordering::Relaxed) as usize
        ));
        vm.call_compiled_with_types(callee, name_of, &[ty], &[args[0]], 0)
    }
    const SOURCE: &str = r#"
        function NameOf<T>(x: T) -> string { "bytecode" }
        function Forward<T>(x: T) -> string { NameOf<T>(x) }
        function Main() -> string { `${Forward<int>(1)}|${Forward<string>("a")}` }
    "#;
    for telemetry in [false, true] {
        let out = run_with_compiled_bodies(
            SOURCE,
            &[
                (
                    "user.NameOf",
                    crate::compiled_rt::CompiledFn::Inline(name_of),
                ),
                (
                    "user.Forward",
                    crate::compiled_rt::CompiledFn::Inline(forward),
                ),
            ],
            &[("user.NameOf", &NAME_OF)],
            telemetry,
        );
        assert_eq!(out, "int|string", "telemetry: {telemetry}");
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn compiled_catch_pops_the_frames_of_compiled_callees() {
    use std::sync::atomic::{AtomicU32, Ordering};
    static WORK: AtomicU32 = AtomicU32::new(0);
    static PANIC: AtomicU32 = AtomicU32::new(0);
    fn work(vm: &mut BexVm, args: &[Value]) -> Result<Value, Box<VmError>> {
        let n = test_int(args[0]);
        Ok(Value::int(vm.div_i64(n, 0)?))
    }
    fn safe(vm: &mut BexVm, args: &[Value]) -> Result<Value, Box<VmError>> {
        let depth = vm.frame_depth();
        let work_ptr = vm.idx_to_ptr(ObjectIndex::from_raw(WORK.load(Ordering::Relaxed) as usize));
        match vm.call_compiled(work_ptr, work, &[args[0]], 0) {
            Ok(v) => Ok(v),
            Err(e) => {
                let (thrown, ctx) = vm.catch(e, depth)?;
                let class =
                    vm.idx_to_ptr(ObjectIndex::from_raw(PANIC.load(Ordering::Relaxed) as usize));
                if vm.is_instance_of(thrown, class) {
                    Ok(Value::int(-1))
                } else {
                    Err(crate::compiled_rt::rethrow(thrown, ctx))
                }
            }
        }
    }
    const SOURCE: &str = r#"
        function Work(n: int) -> int { n / 0 }
        function Safe(n: int) -> int { Work(n) catch (e) { baml.panics.DivisionByZero => -1 } }
        function Main() -> string { `${Safe(5)}|${Safe(6)}|${Safe(7)}` }
    "#;
    for telemetry in [false, true] {
        let out = run_with_compiled_bodies(
            SOURCE,
            &[
                ("user.Work", crate::compiled_rt::CompiledFn::Inline(work)),
                ("user.Safe", crate::compiled_rt::CompiledFn::Inline(safe)),
            ],
            &[("user.Work", &WORK), ("baml.panics.DivisionByZero", &PANIC)],
            telemetry,
        );
        assert_eq!(out, "-1|-1|-1", "telemetry: {telemetry}");
    }
}

// Suspended callers must retain the call instruction PC for traces and telemetry.
#[cfg(not(target_arch = "wasm32"))]
#[test]
fn a_compiled_caller_stands_at_its_call_instruction_while_the_callee_runs() {
    use std::sync::{
        Mutex,
        atomic::{AtomicU32, Ordering},
    };

    use bex_vm_types::bytecode::Instruction;
    static INNER: AtomicU32 = AtomicU32::new(0);
    static MIDDLE: AtomicU32 = AtomicU32::new(0);
    static MIDDLE_CALL: AtomicU32 = AtomicU32::new(0);
    static OUTER_CALL: AtomicU32 = AtomicU32::new(0);
    static SEEN: Mutex<Vec<(usize, usize)>> = Mutex::new(Vec::new());
    #[allow(clippy::unnecessary_wraps)] // the compiled-body signature
    fn inner(vm: &mut BexVm, _args: &[Value]) -> Result<Value, Box<VmError>> {
        let trace = vm.capture_stack_trace();
        let mut seen = SEEN.lock().unwrap();
        seen.clear();
        for name in ["user.Outer", "user.Middle"] {
            let idx = (0..vm.frames.len())
                .rfind(|&idx| {
                    matches!(
                        vm.get_object(vm.frames[idx].function()),
                        Object::Function(function) if function.name() == name
                    )
                })
                .expect("the caller's frame");
            let Frame::Bytecode(frame) = &vm.frames[idx] else {
                unreachable!("a caller has a bytecode frame")
            };
            let pc = vm.compiled_pc(idx).unwrap_or(frame.faulting_pc);
            let line = trace
                .iter()
                .find(|frame| frame.function_name == name)
                .map_or(usize::MAX, |frame| frame.error_line);
            seen.push((pc, line));
        }
        Ok(Value::int(1))
    }
    fn middle(vm: &mut BexVm, _args: &[Value]) -> Result<Value, Box<VmError>> {
        let callee = vm.idx_to_ptr(ObjectIndex::from_raw(INNER.load(Ordering::Relaxed) as usize));
        vm.call_compiled(callee, inner, &[], MIDDLE_CALL.load(Ordering::Relaxed))
    }
    fn outer(vm: &mut BexVm, _args: &[Value]) -> Result<Value, Box<VmError>> {
        let callee = vm.idx_to_ptr(ObjectIndex::from_raw(
            MIDDLE.load(Ordering::Relaxed) as usize
        ));
        vm.call_compiled(callee, middle, &[], OUTER_CALL.load(Ordering::Relaxed))
    }
    const SOURCE: &str = r#"
        function Inner() -> int { 1 }
        function Middle() -> int { Inner() }
        function Outer() -> int { Middle() }
        function Main() -> string { `${Outer()}` }
    "#;
    let program = baml_test_support::compile_source(SOURCE);
    let callables = program.rendered_callables();
    let call_of = |name: &str| {
        let Object::Function(function) = &program.objects[callables[name].object] else {
            unreachable!("a callable is a function")
        };
        let index = function
            .bytecode
            .instructions
            .iter()
            .position(|instruction| matches!(instruction, Instruction::Call { .. }))
            .expect("the caller's call");
        u32::try_from(index).unwrap()
    };
    MIDDLE_CALL.store(call_of("user.Middle"), Ordering::Relaxed);
    OUTER_CALL.store(call_of("user.Outer"), Ordering::Relaxed);
    for telemetry in [false, true] {
        let out = run_with_compiled_bodies(
            SOURCE,
            &[("user.Inner", crate::compiled_rt::CompiledFn::Inline(inner))],
            &[],
            telemetry,
        );
        assert_eq!(out, "1", "telemetry: {telemetry}");
        let bytecode_callers = SEEN.lock().unwrap().clone();
        assert!(
            bytecode_callers
                .iter()
                .all(|&(_, line)| line != 0 && line != usize::MAX),
            "telemetry: {telemetry}: {bytecode_callers:?}"
        );
        let out = run_with_compiled_bodies(
            SOURCE,
            &[
                ("user.Inner", crate::compiled_rt::CompiledFn::Inline(inner)),
                (
                    "user.Middle",
                    crate::compiled_rt::CompiledFn::Inline(middle),
                ),
                ("user.Outer", crate::compiled_rt::CompiledFn::Inline(outer)),
            ],
            &[("user.Inner", &INNER), ("user.Middle", &MIDDLE)],
            telemetry,
        );
        assert_eq!(out, "1", "telemetry: {telemetry}");
        let compiled_callers = SEEN.lock().unwrap().clone();
        assert_eq!(compiled_callers, bytecode_callers, "telemetry: {telemetry}");
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn inline_observed_compiled_calls_record_as_the_call_path_does() {
    use std::{
        fmt::Write,
        sync::atomic::{AtomicU32, Ordering},
    };

    use btel_settings::mode::AutoTelemetryLevel;

    use crate::{
        compiled_rt::FORCE_CALL_FUNNEL,
        telemetry::{InvocationOutcome, SpanRecord, TimingRecord},
    };

    static LEAF: AtomicU32 = AtomicU32::new(0);
    static MID: AtomicU32 = AtomicU32::new(0);
    static REC: AtomicU32 = AtomicU32::new(0);
    static PANIC: AtomicU32 = AtomicU32::new(0);
    fn object(vm: &BexVm, index: &AtomicU32) -> HeapPtr {
        vm.idx_to_ptr(ObjectIndex::from_raw(index.load(Ordering::Relaxed) as usize))
    }
    fn leaf(vm: &mut BexVm, args: &[Value]) -> Result<Value, Box<VmError>> {
        let n = test_int(args[0]);
        if n < 0 {
            return Ok(Value::int(vm.div_i64(n, 0)?));
        }
        Ok(Value::int(n + 1))
    }
    fn mid(vm: &mut BexVm, args: &[Value]) -> Result<Value, Box<VmError>> {
        let leaf_ptr = object(vm, &LEAF);
        let a = vm.call_compiled(leaf_ptr, leaf, &[args[0]], 0)?;
        let b = vm.call_compiled(leaf_ptr, leaf, &[args[0]], 0)?;
        Ok(Value::int(test_int(a) + test_int(b)))
    }
    fn rec(vm: &mut BexVm, args: &[Value]) -> Result<Value, Box<VmError>> {
        let n = test_int(args[0]);
        if n == 0 {
            return Ok(Value::int(0));
        }
        let rec_ptr = object(vm, &REC);
        let below = vm.call_compiled(rec_ptr, rec, &[Value::int(n - 1)], 0)?;
        Ok(Value::int(test_int(below) + 1))
    }
    fn safe(vm: &mut BexVm, args: &[Value]) -> Result<Value, Box<VmError>> {
        let depth = vm.frame_depth();
        let mid_ptr = object(vm, &MID);
        match vm.call_compiled(mid_ptr, mid, &[args[0]], 0) {
            Ok(v) => Ok(v),
            Err(e) => {
                let (thrown, ctx) = vm.catch(e, depth)?;
                let class = object(vm, &PANIC);
                if vm.is_instance_of(thrown, class) {
                    Ok(Value::int(-1))
                } else {
                    Err(crate::compiled_rt::rethrow(thrown, ctx))
                }
            }
        }
    }
    const SOURCE: &str = r#"
        function Leaf(n: int) -> int { if (n < 0) { n / 0 } else { n + 1 } }
        function Mid(n: int) -> int { Leaf(n) + Leaf(n) }
        function Rec(n: int) -> int { if (n == 0) { 0 } else { 1 + Rec(n - 1) } }
        function Safe(n: int) -> int { Mid(n) catch (e) { baml.panics.DivisionByZero => -1 } }
        function Main() -> string { `${Safe(1)}|${Safe(-1)}|${Rec(3)}` }
    "#;

    type NamedPath = (Option<String>, String);

    // Resolve run-specific IDs and discard clock readings before comparison.
    fn records(vm: &BexVm) -> (Vec<(NamedPath, InvocationOutcome, bool)>, Vec<String>) {
        let telemetry = vm.telemetry.as_ref().unwrap();
        let mut paths: HashMap<btel_types::CallPathId, NamedPath> = HashMap::new();
        for event in telemetry.span_records() {
            if let SpanRecord::CallPathDefined {
                call_path,
                visible_caller,
                callee,
                ..
            } = event
            {
                let callee = vm.heap.function_metadata(vm.proof(), *callee).unwrap();
                let caller =
                    visible_caller.map(|id| vm.heap.function_metadata(vm.proof(), id).unwrap().fqn);
                paths.insert(*call_path, (caller, callee.fqn));
            }
        }
        let timings = telemetry
            .timing_records()
            .iter()
            .filter_map(|event| match event {
                TimingRecord::FunctionTimingCompletion {
                    call_path,
                    outcome,
                    reentry,
                    ..
                } => Some((paths[call_path].clone(), *outcome, *reentry)),
                TimingRecord::ThreadSelected { .. } | TimingRecord::SysOpTime { .. } => None,
            })
            .collect();
        let mut ids: HashMap<u64, usize> = HashMap::new();
        let spans = telemetry
            .span_records()
            .iter()
            .filter(|event| !format!("{event:?}").starts_with("Thread"))
            .map(|event| {
                let mut out = String::new();
                let text = format!("{event:?}");
                let mut rest = text.as_str();
                while let Some(open) = rest.find('(') {
                    let (head, tail) = rest.split_at(open + 1);
                    let name = head[..open]
                        .rsplit(|c: char| !c.is_alphanumeric())
                        .next()
                        .unwrap();
                    let close = tail.find(')').unwrap_or(tail.len());
                    let (inner, after) = tail.split_at(close);
                    out.push_str(head);
                    match name {
                        "TelemetryId" => {
                            let raw: u64 = inner.parse().unwrap();
                            let next = ids.len();
                            let _ = write!(out, "#{}", ids.entry(raw).or_insert(next));
                        }
                        "CallPathId" => {
                            let raw: u32 = inner.parse().unwrap();
                            let path = paths.iter().find(|(id, _)| id.get() == raw);
                            match path {
                                Some((_, (caller, callee))) => {
                                    let _ = write!(out, "{caller:?}->{callee}");
                                }
                                None => out.push_str("root"),
                            }
                        }
                        "ClockInstant" | "ClockDuration" => out.push('_'),
                        _ => out.push_str(inner),
                    }
                    rest = after;
                }
                out.push_str(rest);
                out
            })
            .collect();
        (timings, spans)
    }

    for level in [AutoTelemetryLevel::Medium, AutoTelemetryLevel::High] {
        let mut runs = Vec::new();
        for funnel in [true, false] {
            FORCE_CALL_FUNNEL.with(|force| force.set(funnel));
            let (out, vm) = exec_with_compiled_bodies(
                SOURCE,
                &[
                    ("user.Leaf", crate::compiled_rt::CompiledFn::Inline(leaf)),
                    ("user.Mid", crate::compiled_rt::CompiledFn::Inline(mid)),
                    ("user.Rec", crate::compiled_rt::CompiledFn::Inline(rec)),
                    ("user.Safe", crate::compiled_rt::CompiledFn::Inline(safe)),
                ],
                &[
                    ("user.Leaf", &LEAF),
                    ("user.Mid", &MID),
                    ("user.Rec", &REC),
                    ("baml.panics.DivisionByZero", &PANIC),
                ],
                Some(level),
            );
            FORCE_CALL_FUNNEL.with(|force| force.set(false));
            assert_eq!(out, "4|-1|3", "level {level:?}, funnel {funnel}");
            runs.push(records(&vm));
        }
        let (funnel, inline) = (&runs[0], &runs[1]);
        assert_eq!(inline.0, funnel.0, "timing records at {level:?}");
        assert_eq!(inline.1, funnel.1, "span records at {level:?}");

        let named: Vec<_> = inline
            .0
            .iter()
            .map(|((caller, callee), outcome, reentry)| {
                (
                    caller.as_deref().unwrap_or(""),
                    callee.as_str(),
                    *outcome,
                    *reentry,
                )
            })
            .collect();
        let ok = InvocationOutcome::Ok;
        let errored = InvocationOutcome::Panicked;
        let expected = [
            ("user.Mid", "user.Leaf", ok, false),
            ("user.Mid", "user.Leaf", ok, false),
            ("user.Safe", "user.Mid", ok, false),
            ("user.Main", "user.Safe", ok, false),
            ("user.Mid", "user.Leaf", errored, false),
            ("user.Safe", "user.Mid", errored, false),
            ("user.Main", "user.Safe", ok, false),
            ("user.Main", "user.Rec", ok, true),
            ("user.Main", "user.Rec", ok, true),
            ("user.Main", "user.Rec", ok, true),
            ("user.Main", "user.Rec", ok, false),
            ("", "user.Main", ok, false),
        ];
        let user: Vec<_> = named
            .iter()
            .filter(|(_, callee, _, _)| callee.starts_with("user."))
            .copied()
            .collect();
        assert!(
            inline.1.iter().any(|span| span.contains("CallPathDefined")),
            "call paths are defined at {level:?}"
        );
        let completions = |pattern: &str| {
            inline
                .1
                .iter()
                .filter(|span| span.starts_with("FunctionSpanCompletion") && span.contains(pattern))
                .count()
        };
        match level {
            AutoTelemetryLevel::Medium => assert_eq!(user, expected),
            AutoTelemetryLevel::High => {
                assert!(user.is_empty());
                assert_eq!(completions("->user."), expected.len());
                assert_eq!(completions("Panicked"), 2);
                assert_eq!(completions("Reentry"), 3);
            }
            AutoTelemetryLevel::Low => unreachable!(),
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn compiled_bodies_require_the_original_program() {
    use crate::compiled_rt::{CompiledEntry, CompiledFn};

    fn body(_vm: &mut BexVm, _args: &[Value]) -> Result<Value, Box<VmError>> {
        unreachable!("load validation does not execute compiled bodies")
    }

    let program = baml_test_support::compile_source("function Main() -> int { 1 }");
    let object = program.rendered_callables()["user.Main"].object;
    let entries = Box::leak(Box::new([CompiledEntry {
        object: u32::try_from(object.raw()).unwrap(),
        name: "user.Main",
        arity: 0,
        body: CompiledFn::Inline(body),
        slots: 0,
        cause_slot: None,
    }]));
    let artifact = baml_artifact::encode(baml_artifact::ArtifactKind::Program, &program).unwrap();
    let (decoded, digest) =
        baml_artifact::decode_with_digest(baml_artifact::ArtifactKind::Program, &artifact).unwrap();
    let table = Box::leak(Box::new(super::CompiledBodyTable {
        fingerprint: digest,
        entries,
    }));
    let bodies = super::CompiledBodies {
        table,
        artifact_fingerprint: digest,
    };
    let loaded =
        crate::vm::convert_program_with_compiled_bodies(decoded, true, Some(bodies)).unwrap();
    let Object::Function(function) = &loaded.objects[object] else {
        panic!("compiled entry must be a function")
    };
    assert!(function.compiled_body.is_some());

    let changed = baml_test_support::compile_source("function Main() -> int { 2 }");
    let artifact = baml_artifact::encode(baml_artifact::ArtifactKind::Program, &changed).unwrap();
    let (changed, digest) =
        baml_artifact::decode_with_digest(baml_artifact::ArtifactKind::Program, &artifact).unwrap();
    let bodies = super::CompiledBodies {
        table,
        artifact_fingerprint: digest,
    };
    let error = crate::vm::convert_program_with_compiled_bodies(changed, true, Some(bodies))
        .expect_err("same signature with a different body must be refused");
    assert!(error.to_string().contains("fingerprint mismatch"));
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn empty_native_table_still_requires_matching_artifact() {
    static TABLE: super::CompiledBodyTable = super::CompiledBodyTable {
        fingerprint: [0; 32],
        entries: &[],
    };
    let program = baml_test_support::compile_source("function Main() -> int { 1 }");
    let artifact = baml_artifact::encode(baml_artifact::ArtifactKind::Program, &program).unwrap();
    let (program, artifact_fingerprint) =
        baml_artifact::decode_with_digest(baml_artifact::ArtifactKind::Program, &artifact).unwrap();
    let error = crate::vm::convert_program_with_compiled_bodies(
        program,
        true,
        Some(super::CompiledBodies {
            table: &TABLE,
            artifact_fingerprint,
        }),
    )
    .expect_err("an empty table must not bypass artifact validation");
    assert!(error.to_string().contains("fingerprint mismatch"));
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn compiled_entry_points_run_their_bodies() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static RUNS: AtomicUsize = AtomicUsize::new(0);
    #[allow(clippy::unnecessary_wraps)] // the compiled-body signature
    fn main(vm: &mut BexVm, _args: &[Value]) -> Result<Value, Box<VmError>> {
        RUNS.fetch_add(1, Ordering::Relaxed);
        Ok(vm.string_value("compiled"))
    }
    fn failing_main(vm: &mut BexVm, _args: &[Value]) -> Result<Value, Box<VmError>> {
        Ok(Value::int(vm.div_i64(1, 0)?))
    }
    const SOURCE: &str = r#"function Main() -> string { "bytecode" }"#;
    const FAILING: &str = r#"function Main() -> string { "unused" }"#;
    for telemetry in [false, true] {
        let before = RUNS.load(Ordering::Relaxed);
        let out = run_with_compiled_bodies(
            SOURCE,
            &[("user.Main", crate::compiled_rt::CompiledFn::Inline(main))],
            &[],
            telemetry,
        );
        assert_eq!(out, "compiled", "telemetry: {telemetry}");
        assert_eq!(RUNS.load(Ordering::Relaxed), before + 1);
    }
    let out = run_with_compiled_bodies(
        FAILING,
        &[(
            "user.Main",
            crate::compiled_rt::CompiledFn::Inline(failing_main),
        )],
        &[],
        false,
    );
    assert!(
        out.contains("ThrownUnhandled") && out.contains("user.Main"),
        "{out}"
    );
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn resumable_bodies_call_bytecode_and_resume() {
    use std::sync::atomic::{AtomicU32, Ordering};

    use crate::compiled_rt::{CompiledFn, Resume, Step};
    static HELPER: AtomicU32 = AtomicU32::new(0);
    type Out = Result<Step, Box<VmError>>;

    // Slot 1 preserves `k` across the call to bytecode; slot 2 holds the resume point.
    fn twice(vm: &mut BexVm, resume: Resume) -> Out {
        let depth = vm.frame_depth();
        match resume {
            Resume::Start => {
                vm.reserve_slots(depth, 3);
                let n = test_int(vm.slot(depth, 0));
                vm.set_slot(depth, 1, Value::int(n * 10));
                vm.set_slot(depth, 2, Value::int(1));
                let helper = vm.idx_to_ptr(ObjectIndex::from_raw(
                    HELPER.load(Ordering::Relaxed) as usize
                ));
                vm.push_arg(Value::int(n));
                Ok(Step::Call {
                    callee: helper,
                    argc: 1,
                })
            }
            Resume::Value(h) => {
                let k = test_int(vm.slot(depth, 1));
                Ok(Step::Return(Value::int(k + test_int(h))))
            }
            Resume::Thrown(error) => {
                if test_int(vm.slot(depth, 0)) == 0 {
                    let (_thrown, _ctx) = vm.catch(error, depth)?;
                    Ok(Step::Return(Value::int(-1)))
                } else {
                    Err(error)
                }
            }
            Resume::Yielded => unreachable!("Twice never yields"),
        }
    }
    const SOURCE: &str = r#"
        function Helper(n: int) -> int { if (n == 3) { n / 0 } else { 100 / n } }
        function Twice(n: int) -> int { n * 10 + Helper(n) }
        function Main() -> string {
            let caught = Twice(3) catch (e) { baml.panics.DivisionByZero => -9 };
            `${Twice(5)}|${Twice(0)}|${caught}`
        }
    "#;
    for telemetry in [false, true] {
        let out = run_with_compiled_bodies(
            SOURCE,
            &[("user.Twice", CompiledFn::Resumable(twice))],
            &[("user.Helper", &HELPER)],
            telemetry,
        );
        assert_eq!(out, "70|-1|-9", "telemetry: {telemetry}");
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn compiled_delivery_survives_collection_before_resume() {
    use crate::{
        compiled_rt::{CompiledFn, Resume, Step},
        errors::VmError,
    };

    fn body(vm: &mut BexVm, resume: Resume) -> Result<Step, Box<VmError>> {
        match resume {
            Resume::Start => Ok(Step::Yield),
            Resume::Thrown(error) => {
                let (value, context) = vm.catch(error, vm.frame_depth())?;
                let Object::Instance(context) = vm.get_object(context.as_object_ptr().unwrap())
                else {
                    panic!("expected error context");
                };
                assert_eq!(context.load_field(0), value);
                assert_eq!(vm.as_string(&value).unwrap(), "delivered error");
                Ok(Step::Return(value))
            }
            _ => panic!("expected a delivered exception"),
        }
    }

    let mut program =
        baml_test_support::compile_source(r#"function Main() -> string { "unused" }"#);
    let entry = program.rendered_callables()["user.Main"].object;
    let Object::Function(function) = &mut program.objects[entry] else {
        unreachable!()
    };
    function.compiled_body = Some(test_body(CompiledFn::Resumable(body), 0));
    let mut vm = BexVm::from_program(program, Arc::new(AtomicBool::new(false))).unwrap();
    vm.telemetry = None;
    vm.set_entry_point(vm.heap.compile_time_ptr(entry.raw()), &[]);
    assert!(matches!(vm.exec().unwrap(), VmExecState::EarlyYield));

    let thrown_ptr = vm.string_value("delivered error").as_object_ptr().unwrap();
    vm.try_handle_external_exception(Value::object(thrown_ptr))
        .unwrap();
    let Some(VmError::Thrown(thrown)) = vm.compiled.delivery.as_deref() else {
        panic!("the unwinder must deliver to the suspended body");
    };
    let context_ptr = thrown.context.unwrap().as_object_ptr().unwrap();
    let mut roots = Vec::new();
    vm.collect_roots(&mut roots);
    assert!(roots.contains(&thrown_ptr));
    assert!(roots.contains(&context_ptr));

    // SAFETY: this test owns the only VM and keeps it suspended until its roots are forwarded.
    let (_, _, forwarding) = unsafe {
        vm.heap
            .collect_garbage_generational(&roots, CollectionLevel::Major)
    };
    let moved_value = forwarding[&thrown_ptr];
    let moved_context = forwarding[&context_ptr];
    assert_ne!(moved_value, thrown_ptr);
    assert_ne!(moved_context, context_ptr);
    vm.forward_roots(&forwarding);
    let Some(VmError::Thrown(thrown)) = vm.compiled.delivery.as_deref() else {
        unreachable!()
    };
    assert_eq!(thrown.value, Value::object(moved_value));
    assert_eq!(thrown.context, Some(Value::object(moved_context)));
    assert!(matches!(
        vm.exec().unwrap(),
        VmExecState::Complete(value) if value == Value::object(moved_value)
    ));
    assert!(vm.compiled.delivery.is_none());
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn resumable_bodies_call_bound_method_values() {
    use crate::compiled_rt::{CompiledFn, Resume, Step};
    type Out = Result<Step, Box<VmError>>;

    fn apply(vm: &mut BexVm, resume: Resume) -> Out {
        let depth = vm.frame_depth();
        match resume {
            Resume::Start => {
                vm.reserve_slots(depth, 3);
                let callee = crate::compiled_rt::callee_ptr(vm.slot(depth, 0))?;
                let x = vm.slot(depth, 1);
                vm.set_slot(depth, 2, Value::int(1));
                vm.push_arg(x);
                Ok(Step::Call { callee, argc: 1 })
            }
            Resume::Value(value) => Ok(Step::Return(value)),
            Resume::Thrown(error) => Err(error),
            Resume::Yielded => unreachable!("Apply never yields"),
        }
    }
    const SOURCE: &str = r#"
        class Scale {
            by int
            function times(self, x: int) -> int { self.by * x }
        }
        function Apply(f: (int) -> int throws never, x: int) -> int { f(x) }
        function Main() -> string {
            let s = Scale { by: 3 };
            `${Apply(s.times, 5)}|${Apply((x: int) -> int { x + 1 }, 5)}`
        }
    "#;
    for telemetry in [false, true] {
        let out = run_with_compiled_bodies(
            SOURCE,
            &[("user.Apply", CompiledFn::Resumable(apply))],
            &[],
            telemetry,
        );
        assert_eq!(out, "15|6", "telemetry: {telemetry}");
    }
}

// Compiled frames must never use exception handlers from their replaced bytecode.
#[cfg(not(target_arch = "wasm32"))]
#[test]
fn errors_leave_compiled_frames_past_their_bytecode_handlers() {
    use std::sync::atomic::{AtomicU32, Ordering};
    static WORK: AtomicU32 = AtomicU32::new(0);
    fn work(vm: &mut BexVm, args: &[Value]) -> Result<Value, Box<VmError>> {
        let n = test_int(args[0]);
        Ok(Value::int(vm.div_i64(n, 0)?))
    }
    // Safe's bytecode catches this error; its compiled body lets it escape.
    fn safe(vm: &mut BexVm, args: &[Value]) -> Result<Value, Box<VmError>> {
        let work_ptr = vm.idx_to_ptr(ObjectIndex::from_raw(WORK.load(Ordering::Relaxed) as usize));
        vm.call_compiled(work_ptr, work, &[args[0]], 0)
    }
    const SOURCE: &str = r#"
        function Work(n: int) -> int { n / 0 }
        function Safe(n: int) -> int { Work(n) catch (e) { baml.panics.DivisionByZero => -1 } }
        function Main() -> string {
            let r = Safe(1) catch (e) { baml.panics.DivisionByZero => -9 };
            `${r}`
        }
    "#;
    for telemetry in [false, true] {
        let out = run_with_compiled_bodies(
            SOURCE,
            &[
                ("user.Work", crate::compiled_rt::CompiledFn::Inline(work)),
                ("user.Safe", crate::compiled_rt::CompiledFn::Inline(safe)),
            ],
            &[("user.Work", &WORK)],
            telemetry,
        );
        assert_eq!(out, "-9", "telemetry: {telemetry}");
    }
}

// Rethrowing must preserve the original throw context across compiled and bytecode frames.
#[cfg(not(target_arch = "wasm32"))]
#[test]
fn compiled_rethrow_keeps_the_original_context() {
    use std::sync::atomic::{AtomicU32, Ordering};
    static WORK: AtomicU32 = AtomicU32::new(0);
    fn work(vm: &mut BexVm, args: &[Value]) -> Result<Value, Box<VmError>> {
        let n = test_int(args[0]);
        Ok(Value::int(vm.div_i64(n, 0)?))
    }
    fn relay(vm: &mut BexVm, args: &[Value]) -> Result<Value, Box<VmError>> {
        let depth = vm.frame_depth();
        let work_ptr = vm.idx_to_ptr(ObjectIndex::from_raw(WORK.load(Ordering::Relaxed) as usize));
        match vm.call_compiled(work_ptr, work, &[args[0]], 0) {
            Ok(v) => Ok(v),
            Err(e) => {
                let (thrown, ctx) = vm.catch(e, depth)?;
                assert!(!ctx.is_null(), "a compiled catch lands a real context");
                Err(crate::compiled_rt::rethrow(thrown, ctx))
            }
        }
    }
    const SOURCE: &str = r#"
        function Work(n: int) -> int { n / 0 }
        function Relay(n: int) -> int { Work(n) catch (e) { _ => { throw e } } }
        function Main() -> string {
            let name = "none";
            let r = Relay(1) catch (e, ctx) {
                baml.panics.DivisionByZero => {
                    let frames = ctx.stack_trace.frames;
                    name = frames[frames.length() - 1].function_name;
                    -1
                }
            };
            name
        }
    "#;
    let bytecode = run_with_compiled_bodies(SOURCE, &[], &[], true);
    assert_eq!(bytecode, "user.Work");
    for telemetry in [false, true] {
        let out = run_with_compiled_bodies(
            SOURCE,
            &[
                ("user.Work", crate::compiled_rt::CompiledFn::Inline(work)),
                ("user.Relay", crate::compiled_rt::CompiledFn::Inline(relay)),
            ],
            &[("user.Work", &WORK)],
            telemetry,
        );
        assert_eq!(out, bytecode, "telemetry: {telemetry}");
    }
}

#[test]
fn call_type_lanes_root_and_forward_runtime_heads() {
    for lane in 0..5 {
        let (mut vm, native_ptr) = crate::vm::tests::vm_with_native_entry();
        vm.set_entry_point(native_ptr, &[]);

        let tag = baml_type::typetag::TypeTag::fresh_dynamic();
        let class_ptr = vm.tlab.alloc(Object::Class(Box::new(bex_vm_types::Class {
            path: bex_vm_types::types::LocalName::new(
                Vec::new(),
                baml_type::Name::new("PendingNativeArg"),
            ),
            fields: Vec::new(),
            description: None,
            alias: None,
            docstring: None,
            other: indexmap::IndexMap::new(),
            stream_done: false,
            type_tag: tag,
            cleanup: None,
            methods: indexmap::IndexMap::new(),
            generic_param_count: 0,
            owner: bex_vm_types::types::Owner::anonymous(),
        })));
        let ty = bex_vm_types::RealizedTy::Class(
            bex_vm_types::TypeHead::new(class_ptr, tag),
            Box::new([]),
        );
        match lane {
            0 => vm.pending_call_type_args.push(ty),
            1 => vm.compiled.call_type_args.push(ty),
            2 => vm.compiled.builtin_type_args.push(ty),
            3 => {
                vm.pending_call_type_args.push(ty);
                vm.save_call_types();
            }
            _ => {
                vm.pending_call_type_values
                    .push(Some(crate::vm::TypeValue::new(ty)));
                vm.save_call_types();
            }
        }

        let mut roots = Vec::new();
        vm.collect_roots(&mut roots);
        assert!(
            roots.contains(&class_ptr),
            "a native call's realized argument must root its runtime declaration",
        );

        let (_stats, _remapped_roots, forwarding) = unsafe {
            vm.heap
                .collect_garbage_generational(&roots, CollectionLevel::Major)
        };
        let moved = forwarding
            .get(&class_ptr)
            .copied()
            .expect("the pending native type argument must survive collection");
        vm.forward_roots(&forwarding);

        let ty = match lane {
            0 => &vm.pending_call_type_args[0],
            1 => &vm.compiled.call_type_args[0],
            2 => &vm.compiled.builtin_type_args[0],
            3 => {
                vm.restore_call_types();
                &vm.pending_call_type_args[0]
            }
            _ => {
                vm.restore_call_types();
                &vm.pending_call_type_values[0].as_ref().unwrap().ty
            }
        };
        let bex_vm_types::RealizedTy::Class(head, ..) = ty else {
            panic!("the call argument must remain the dynamic class type")
        };
        assert_eq!(head.ptr(), moved);
    }
}
