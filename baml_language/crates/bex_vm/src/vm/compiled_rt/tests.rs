use std::{
    collections::HashMap,
    sync::{Arc, atomic::AtomicBool},
};

use bex_heap::CollectionLevel;
use bex_vm_types::{FunctionKind, HeapPtr, Object, ObjectIndex, RootHaver, Value};

use crate::{
    errors::VmError,
    telemetry::{InvocationOutcome, SpanRecord, TelemetryPolicies, TelemetryState, TimingRecord},
    vm::{BexVm, Frame, VmExecState},
};

// Rust tests can install compiled bodies directly.
#[cfg(not(target_arch = "wasm32"))]
#[test]
fn compiled_body_is_indistinguishable_from_bytecode() {
    use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};

    static CALLS: AtomicUsize = AtomicUsize::new(0);
    static SAME: AtomicU32 = AtomicU32::new(0);
    fn add_compiled(vm: &mut BexVm, args: &[Value]) -> Result<Value, Box<VmError>> {
        CALLS.fetch_add(1, Ordering::Relaxed);
        let sum = vm.add_i64(test_int(args[0]), test_int(args[1]))?;
        Ok(Value::int(sum))
    }
    // `Same<T>` forwards its type argument to the compiled `Add` body.
    fn same_compiled(vm: &mut BexVm, args: &[Value]) -> Result<Value, Box<VmError>> {
        let depth = vm.frame_depth();
        let t = vm.frame_type_arg(depth, 0)?;
        let callee = vm.idx_to_ptr(ObjectIndex::from_raw(SAME.load(Ordering::Relaxed) as usize));
        let _ = vm.call_compiled(
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
                ("user.Add", add_compiled as crate::compiled_rt::CompiledFn),
                ("user.Same", same_compiled),
            ] {
                let object = program.rendered_callables()[name].object.raw();
                let Object::Function(function) =
                    &mut program.objects[ObjectIndex::from_raw(object)]
                else {
                    unreachable!()
                };
                assert!(matches!(function.kind, FunctionKind::Bytecode));
                function.compiled_body = Some(test_body(body, 0));
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
        // Generated bodies reserve at least their arguments' slots.
        let slots = u32::try_from(function.arity).unwrap();
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
            // The engine answers a log with a null.
            Ok(VmExecState::Log { .. }) => vm.stack.push(Value::NULL),
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
#[allow(unsafe_code)]
fn a_body_paused_at_a_yield_survives_a_moving_collection() {
    use crate::compiled_rt::Step;

    fn body(vm: &mut BexVm, _: &[Value]) -> Result<Value, Box<VmError>> {
        let depth = vm.frame_depth();
        vm.reserve_slots(depth, 1);
        let mut live = vm.string_value("still live");
        for _ in 0..3 {
            let before = live;
            vm.set_slot(depth, 0, live);
            vm.suspend(Step::Yield)?;
            live = vm.slot(depth, 0);
            assert_ne!(live, before, "the collection moves the value");
            assert_eq!(vm.as_string(&live).unwrap().as_str(), "still live");
        }
        Ok(live)
    }
    let mut program = baml_test_support::compile_source("function Main() -> string { \"\" }");
    let entry = program.rendered_callables()["user.Main"].object;
    let Object::Function(function) = &mut program.objects[entry] else {
        unreachable!()
    };
    function.compiled_body = Some(test_body(body, 1));
    let mut vm = BexVm::from_program(program, Arc::new(AtomicBool::new(false))).unwrap();
    vm.set_entry_point(vm.heap.compile_time_ptr(entry.raw()), &[]);
    let out = loop {
        match vm.exec().unwrap() {
            VmExecState::EarlyYield => {
                let mut roots = Vec::new();
                vm.collect_roots(&mut roots);
                // SAFETY: this VM is the heap's only mutator and it is parked.
                let (_, _, forwarding) = unsafe {
                    vm.heap
                        .collect_garbage_generational(&roots, CollectionLevel::Major)
                };
                vm.forward_roots(&forwarding);
            }
            VmExecState::Complete(value) => break value,
            other => panic!("unexpected execution state: {other:?}"),
        }
    };
    assert_eq!(vm.as_string(&out).unwrap().as_str(), "still live");
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
        let below = vm.call_compiled(me, deep, &[], &[Value::int(n - 1)], 0)?;
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
            &[("user.Deep", deep)],
            &[("user.Deep", &DEEP)],
            telemetry,
        );
        assert_eq!(out, "100|-7|-8", "telemetry: {telemetry}");
    }
}

// Large frames exhaust the fiber stack before the VM reaches its frame limit.
#[cfg(not(target_arch = "wasm32"))]
#[test]
fn compiled_recursion_checks_fiber_stack_space() {
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
        let below = vm.call_compiled(me, deep, &[], &[Value::int(n - 1)], 0)?;
        let one = std::hint::black_box(&pad)[slot];
        Ok(Value::int(test_int(below) + i64::try_from(one).unwrap()))
    }
    const SOURCE: &str = r#"
        function Deep(n: int) -> int { if (n == 0) { 0 } else { 1 + Deep(n - 1) } }
        function Main() -> string {
            let shallow = Deep(10);
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
                &[("user.Deep", deep)],
                &[("user.Deep", &DEEP)],
                telemetry,
            );
            assert_eq!(out, "10|-7|-8", "telemetry: {telemetry}");
            assert!(
                (11..crate::vm::MAX_FRAMES).contains(&DEEPEST.load(Ordering::Relaxed)),
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

// A body entered from bytecode nests on the fiber below the body that called
// that bytecode, so it must check for stack room as a direct call does.
#[cfg(not(target_arch = "wasm32"))]
#[test]
fn compiled_bytecode_recursion_checks_fiber_stack_space() {
    use std::sync::atomic::{AtomicU32, Ordering};

    use crate::compiled_rt::Step;
    static PONG: AtomicU32 = AtomicU32::new(0);
    fn ping(vm: &mut BexVm, args: &[Value]) -> Result<Value, Box<VmError>> {
        let n = test_int(args[0]);
        if n == 0 {
            return Ok(Value::int(0));
        }
        let mut pad = [0u64; 8192];
        let slot = usize::try_from(n).unwrap() % pad.len();
        pad[slot] = 1;
        std::hint::black_box(&mut pad);
        let pong = vm.idx_to_ptr(ObjectIndex::from_raw(PONG.load(Ordering::Relaxed) as usize));
        vm.push_arg(Value::int(n - 1));
        let below = vm.suspend(Step::Call {
            callee: pong,
            argc: 1,
        })?;
        let one = std::hint::black_box(&pad)[slot];
        Ok(Value::int(test_int(below) + i64::try_from(one).unwrap()))
    }
    const SOURCE: &str = r#"
        function Ping(n: int) -> int { if (n == 0) { 0 } else { 1 + Pong(n - 1) } }
        function Pong(n: int) -> int { Ping(n) }
        function Main() -> string {
            let shallow = Ping(10);
            let huge = Ping(1000000) catch (e) { baml.panics.StackOverflow => -8 };
            `${shallow}|${huge}`
        }
    "#;
    for telemetry in [false, true] {
        let out = run_with_compiled_bodies(
            SOURCE,
            &[("user.Ping", ping)],
            &[("user.Pong", &PONG)],
            telemetry,
        );
        assert_eq!(out, "10|-8", "telemetry: {telemetry}");
    }
}

// The engine pushes a null after every log; a compiled body must discard it.
#[cfg(not(target_arch = "wasm32"))]
#[test]
fn compiled_logs_leave_the_stack_as_they_found_it() {
    use std::sync::Mutex;

    static LENGTHS: Mutex<Vec<usize>> = Mutex::new(Vec::new());
    fn body(vm: &mut BexVm, _: &[Value]) -> Result<Value, Box<VmError>> {
        for _ in 0..4 {
            vm.log_event(
                crate::compiled_rt::LogLevel::Info,
                Value::int(1),
                Value::NULL,
                0,
            )?;
            LENGTHS.lock().unwrap().push(vm.stack.len());
        }
        Ok(vm.string_value("done"))
    }
    let out = run_with_compiled_bodies(
        r#"function Main() -> string { "" }"#,
        &[("user.Main", body)],
        &[],
        false,
    );
    assert_eq!(out, "done");
    let lengths = LENGTHS.lock().unwrap();
    assert_eq!(lengths.len(), 4);
    assert!(lengths.iter().all(|&len| len == lengths[0]), "{lengths:?}");
}

// Type arguments must be bound before a body runs in place, including when it throws.
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
        let out = run_with_compiled_bodies(SOURCE, &[("user.NameOf", name_of)], &[], telemetry);
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
        vm.call_compiled(callee, name_of, &[ty], &[args[0]], 0)
    }
    const SOURCE: &str = r#"
        function NameOf<T>(x: T) -> string { "bytecode" }
        function Forward<T>(x: T) -> string { NameOf<T>(x) }
        function Main() -> string { `${Forward<int>(1)}|${Forward<string>("a")}` }
    "#;
    for telemetry in [false, true] {
        let out = run_with_compiled_bodies(
            SOURCE,
            &[("user.NameOf", name_of), ("user.Forward", forward)],
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
        match vm.call_compiled(work_ptr, work, &[], &[args[0]], 0) {
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
            &[("user.Work", work), ("user.Safe", safe)],
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
        vm.call_compiled(callee, inner, &[], &[], MIDDLE_CALL.load(Ordering::Relaxed))
    }
    fn outer(vm: &mut BexVm, _args: &[Value]) -> Result<Value, Box<VmError>> {
        let callee = vm.idx_to_ptr(ObjectIndex::from_raw(
            MIDDLE.load(Ordering::Relaxed) as usize
        ));
        vm.call_compiled(callee, middle, &[], &[], OUTER_CALL.load(Ordering::Relaxed))
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
        let out = run_with_compiled_bodies(SOURCE, &[("user.Inner", inner)], &[], telemetry);
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
                ("user.Inner", inner),
                ("user.Middle", middle),
                ("user.Outer", outer),
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
type NamedPath = (Option<String>, String);

// Resolve run-specific IDs and discard clock readings before comparison.
fn telemetry_records(vm: &BexVm) -> (Vec<(NamedPath, InvocationOutcome, bool)>, Vec<String>) {
    use std::fmt::Write as _;

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

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn compiled_bodies_require_the_original_program() {
    use crate::compiled_rt::CompiledEntry;

    fn body(_vm: &mut BexVm, _args: &[Value]) -> Result<Value, Box<VmError>> {
        unreachable!("load validation does not execute compiled bodies")
    }

    let program = baml_test_support::compile_source("function Main() -> int { 1 }");
    let object = program.rendered_callables()["user.Main"].object;
    let entries = Box::leak(Box::new([CompiledEntry {
        object: u32::try_from(object.raw()).unwrap(),
        name: "user.Main",
        arity: 0,
        body,
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
        let out = run_with_compiled_bodies(SOURCE, &[("user.Main", main)], &[], telemetry);
        assert_eq!(out, "compiled", "telemetry: {telemetry}");
        assert_eq!(RUNS.load(Ordering::Relaxed), before + 1);
    }
    let out = run_with_compiled_bodies(FAILING, &[("user.Main", failing_main)], &[], false);
    assert!(
        out.contains("ThrownUnhandled") && out.contains("user.Main"),
        "{out}"
    );
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn compiled_bodies_call_bytecode_and_resume() {
    use std::sync::atomic::{AtomicU32, Ordering};

    use crate::compiled_rt::Step;
    static HELPER: AtomicU32 = AtomicU32::new(0);

    fn twice(vm: &mut BexVm, args: &[Value]) -> Result<Value, Box<VmError>> {
        let depth = vm.frame_depth();
        let n = test_int(args[0]);
        let helper = vm.idx_to_ptr(ObjectIndex::from_raw(
            HELPER.load(Ordering::Relaxed) as usize
        ));
        vm.push_arg(Value::int(n));
        match vm.suspend(Step::Call {
            callee: helper,
            argc: 1,
        }) {
            Ok(h) => Ok(Value::int(n * 10 + test_int(h))),
            Err(error) if n == 0 => {
                vm.catch(error, depth)?;
                Ok(Value::int(-1))
            }
            Err(error) => Err(error),
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
            &[("user.Twice", twice)],
            &[("user.Helper", &HELPER)],
            telemetry,
        );
        assert_eq!(out, "70|-1|-9", "telemetry: {telemetry}");
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn compiled_delivery_survives_collection_before_resume() {
    use crate::compiled_rt::Step;

    fn body(vm: &mut BexVm, _args: &[Value]) -> Result<Value, Box<VmError>> {
        let error = vm
            .suspend(Step::Yield)
            .expect_err("expected a delivered exception");
        let (value, context) = vm.catch(error, vm.frame_depth())?;
        let Object::Instance(context) = vm.get_object(context.as_object_ptr().unwrap()) else {
            panic!("expected error context");
        };
        assert_eq!(context.load_field(0), value);
        assert_eq!(vm.as_string(&value).unwrap(), "delivered error");
        Ok(value)
    }

    let mut program =
        baml_test_support::compile_source(r#"function Main() -> string { "unused" }"#);
    let entry = program.rendered_callables()["user.Main"].object;
    let Object::Function(function) = &mut program.objects[entry] else {
        unreachable!()
    };
    function.compiled_body = Some(test_body(body, 0));
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
fn compiled_bodies_call_bound_method_values() {
    use crate::compiled_rt::Step;

    fn apply(vm: &mut BexVm, args: &[Value]) -> Result<Value, Box<VmError>> {
        let callee = crate::compiled_rt::callee_ptr(args[0])?;
        vm.push_arg(args[1]);
        vm.suspend(Step::Call { callee, argc: 1 })
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
        let out = run_with_compiled_bodies(SOURCE, &[("user.Apply", apply)], &[], telemetry);
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
        vm.call_compiled(work_ptr, work, &[], &[args[0]], 0)
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
            &[("user.Work", work), ("user.Safe", safe)],
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
        match vm.call_compiled(work_ptr, work, &[], &[args[0]], 0) {
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
            &[("user.Work", work), ("user.Relay", relay)],
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

/// A compiled body with a declaration hook resolves the hook the call path
/// left pending as its bytecode prologue does: the built-in modes through
/// `builtin_trace_hook`, a hook function through `begin_trace_hook`, a call on
/// the VM's call path, and `finish_trace_hook`. The records match bytecode's
/// whether the hooked callee is entered from bytecode or from a compiled
/// caller, and a hook that panics falls back to an ordinary entry.
#[cfg(not(target_arch = "wasm32"))]
#[test]
fn compiled_trace_hooks_record_as_bytecode_does() {
    use std::sync::atomic::{AtomicU32, Ordering};

    use btel_settings::mode::AutoTelemetryLevel;

    use crate::compiled_rt::{Step, TraceHookMode};

    static HIDDEN: AtomicU32 = AtomicU32::new(0);
    static TIMING: AtomicU32 = AtomicU32::new(0);
    static SPAN: AtomicU32 = AtomicU32::new(0);
    static EMPTY: AtomicU32 = AtomicU32::new(0);
    static HOOKED: AtomicU32 = AtomicU32::new(0);
    static FALLBACK: AtomicU32 = AtomicU32::new(0);
    static DECIDE: AtomicU32 = AtomicU32::new(0);
    static BOOM: AtomicU32 = AtomicU32::new(0);
    fn object(vm: &BexVm, index: &AtomicU32) -> HeapPtr {
        vm.idx_to_ptr(ObjectIndex::from_raw(index.load(Ordering::Relaxed) as usize))
    }
    fn mode(
        vm: &mut BexVm,
        n: Value,
        mode: TraceHookMode,
        add: i64,
    ) -> Result<Value, Box<VmError>> {
        let depth = vm.frame_depth();
        vm.builtin_trace_hook(depth, mode)?;
        Ok(Value::int(test_int(n) + add))
    }
    fn hidden(vm: &mut BexVm, args: &[Value]) -> Result<Value, Box<VmError>> {
        mode(vm, args[0], TraceHookMode::Hidden, 1)
    }
    fn timing(vm: &mut BexVm, args: &[Value]) -> Result<Value, Box<VmError>> {
        mode(vm, args[0], TraceHookMode::Timing, 2)
    }
    fn span(vm: &mut BexVm, args: &[Value]) -> Result<Value, Box<VmError>> {
        mode(vm, args[0], TraceHookMode::Span, 3)
    }
    fn empty(vm: &mut BexVm, args: &[Value]) -> Result<Value, Box<VmError>> {
        mode(vm, args[0], TraceHookMode::EmptySpan, 4)
    }
    // The prologue MIR lowers a declaration hook to: settings =
    // TraceHookSettings; if settings != null { result = hook(n, settings);
    // ApplyTraceHook(result) }.
    fn with_hook(
        vm: &mut BexVm,
        n: Value,
        hook: &AtomicU32,
        add: i64,
    ) -> Result<Value, Box<VmError>> {
        let depth = vm.frame_depth();
        let settings = vm.begin_trace_hook(depth, true)?;
        if !settings.is_null() {
            let callee = object(vm, hook);
            vm.push_arg(n);
            vm.push_arg(settings);
            let result = vm.suspend(Step::Call { callee, argc: 2 })?;
            vm.finish_trace_hook(depth, result)?;
        }
        Ok(Value::int(test_int(n) + add))
    }
    fn hooked(vm: &mut BexVm, args: &[Value]) -> Result<Value, Box<VmError>> {
        with_hook(vm, args[0], &DECIDE, 5)
    }
    fn fallback(vm: &mut BexVm, args: &[Value]) -> Result<Value, Box<VmError>> {
        with_hook(vm, args[0], &BOOM, 6)
    }
    fn mid(vm: &mut BexVm, args: &[Value]) -> Result<Value, Box<VmError>> {
        let n = args[0];
        let mut total = 0;
        for (index, body) in [
            (&HIDDEN, hidden as crate::compiled_rt::CompiledFn),
            (&TIMING, timing),
            (&SPAN, span),
            (&EMPTY, empty),
            (&HOOKED, hooked),
            (&FALLBACK, fallback),
        ] {
            let callee = object(vm, index);
            total += test_int(vm.call_compiled(callee, body, &[], &[n], 0)?);
        }
        Ok(Value::int(total))
    }
    const SOURCE: &str = r#"
        /// baml:$trace=trace.hidden
        function Hidden(n: int) -> int { n + 1 }
        /// baml:$trace=trace.timing
        function Timing(n: int) -> int { n + 2 }
        /// baml:$trace=trace.span
        function Span(n: int) -> int { n + 3 }
        /// baml:$trace=trace.empty_span
        function Empty(n: int) -> int { n + 4 }
        function Decide(n: int, settings: trace.Settings) -> trace.Options? throws never {
            trace.span(inputs = true).context(metadata = { "decided": "yes" })
        }
        /// baml:$trace=Decide
        function Hooked(n: int) -> int { n + 5 }
        function Boom(n: int, settings: trace.Settings) -> trace.Options? throws never {
            baml.sys.panic("hook failure")
        }
        /// baml:$trace=Boom
        function Fallback(n: int) -> int { n + 6 }
        function Mid(n: int) -> int {
            Hidden(n) + Timing(n) + Span(n) + Empty(n) + Hooked(n) + Fallback(n)
        }
        function Main() -> string {
            `${Mid(1)}|${Hidden(2)}|${Timing(2)}|${Span(2)}|${Empty(2)}|${Hooked(2)}|${Fallback(2)}`
        }
    "#;
    const BODIES: &[(&str, crate::compiled_rt::CompiledFn)] = &[
        ("user.Hidden", hidden),
        ("user.Timing", timing),
        ("user.Span", span),
        ("user.Empty", empty),
        ("user.Hooked", hooked),
        ("user.Fallback", fallback),
        ("user.Mid", mid),
    ];
    const OBJECTS: &[(&str, &AtomicU32)] = &[
        ("user.Hidden", &HIDDEN),
        ("user.Timing", &TIMING),
        ("user.Span", &SPAN),
        ("user.Empty", &EMPTY),
        ("user.Hooked", &HOOKED),
        ("user.Fallback", &FALLBACK),
        ("user.Decide", &DECIDE),
        ("user.Boom", &BOOM),
    ];
    const EXPECTED: &str = "27|3|4|5|6|7|8";

    for level in [
        None,
        Some(AutoTelemetryLevel::Medium),
        Some(AutoTelemetryLevel::High),
    ] {
        let (out, bytecode) = exec_with_compiled_bodies(SOURCE, &[], OBJECTS, level);
        assert_eq!(out, EXPECTED, "bytecode at {level:?}");
        let (out, compiled) = exec_with_compiled_bodies(SOURCE, BODIES, OBJECTS, level);
        assert_eq!(out, EXPECTED, "compiled at {level:?}");
        assert!(bytecode.pending_trace_hooks.is_empty());
        assert!(compiled.pending_trace_hooks.is_empty());
        if level.is_none() {
            continue;
        }
        let bytecode = telemetry_records(&bytecode);
        let compiled = telemetry_records(&compiled);
        assert_eq!(compiled.0, bytecode.0, "timing records at {level:?}");
        // The hand-written `mid` passes no call instruction (PC 0); generated
        // bodies pass theirs (see `a_compiled_call_passes_its_call_instruction`).
        let without_mid_pcs = |spans: &[String]| -> Vec<String> {
            spans
                .iter()
                .map(|span| {
                    if !span.contains("Some(\"user.Mid\")->") {
                        return span.clone();
                    }
                    let Some(at) = span.find("caller_pc: ") else {
                        return span.clone();
                    };
                    let digits = span[at + 11..]
                        .find(|c: char| !c.is_ascii_digit())
                        .unwrap_or(span.len() - at - 11);
                    format!("{}caller_pc: _{}", &span[..at], &span[at + 11 + digits..])
                })
                .collect()
        };
        assert_eq!(
            without_mid_pcs(&compiled.1),
            without_mid_pcs(&bytecode.1),
            "span records at {level:?}"
        );
        // The modes took effect: hidden and timing callees complete no span,
        // spans and the hook's decision do, and both hooks ran.
        let spans = |pattern: &str| {
            compiled
                .1
                .iter()
                .filter(|span| span.starts_with("FunctionSpanCompletion") && span.contains(pattern))
                .count()
        };
        assert_eq!(spans("->user.Hidden"), 0, "{:#?}", compiled.1);
        assert_eq!(spans("->user.Timing"), 0, "{:#?}", compiled.1);
        assert_eq!(spans("->user.Span"), 2, "{:#?}", compiled.1);
        assert_eq!(spans("->user.Hooked"), 2, "{:#?}", compiled.1);
        for callee in ["->user.Decide)", "->user.Boom)", "->user.Fallback)"] {
            assert!(
                compiled.1.iter().any(|span| span.contains(callee)),
                "{callee}: {:#?}",
                compiled.1
            );
        }
    }
}
