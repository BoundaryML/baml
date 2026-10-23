use std::{
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU32, Ordering},
    },
};

use bex_heap::CollectionLevel;
use bex_vm_types::{FunctionKind, HeapPtr, Object, ObjectIndex, RootHaver, Value};

use super::{CompiledFn, FrameRecord};

type BodyResult = Result<Value, Box<VmError>>;
use crate::{
    errors::VmError,
    telemetry::{InvocationOutcome, SpanRecord, TelemetryPolicies, TelemetryState, TimingRecord},
    vm::{BexVm, Frame, VmExecState},
};

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn compiled_generic_calls_capture_type_arguments_as_bytecode_does() {
    static SAME: AtomicU32 = AtomicU32::new(0);
    fn same(vm: &mut BexVm, fr: &FrameRecord, args: &[Value]) -> BodyResult {
        let ty = vm.frame_type_arg(fr, 0)?;
        Ok(Value::bool(vm.value_is_member_of(args[0], &ty)))
    }
    fn forward(vm: &mut BexVm, fr: &FrameRecord, args: &[Value]) -> BodyResult {
        let ty = vm.frame_type_arg(fr, 0)?;
        vm.call_compiled(fr, object(vm, &SAME), same, &[ty], args, 0)
    }
    const SOURCE: &str = r#"
        function Same<T>(x: T) -> bool { x is T }
        function Forward<T>(x: T) -> bool { Same<T>(x) }
        function Main() -> string { `${Forward<int>(1)}|${Forward<string>("a")}|${Forward([1])}` }
    "#;
    let mut runs = Vec::new();
    for bodies in [
        &[][..],
        &[("user.Same", same as CompiledFn), ("user.Forward", forward)][..],
    ] {
        let program = compiled_program(SOURCE, bodies, &[("user.Same", &SAME)]);
        let entry = program.rendered_callables()["user.Main"].object.raw();
        let mut vm = BexVm::from_program(program, Arc::new(AtomicBool::new(false))).unwrap();
        let Object::Function(function) = vm.get_object(object(&vm, &SAME)) else {
            unreachable!()
        };
        vm.telemetry
            .as_ref()
            .unwrap()
            .set_policy(
                function,
                crate::telemetry::TelemetryPolicy {
                    span_from_entry: true,
                    capture_inputs: true,
                    ..crate::telemetry::TelemetryPolicy::NONE
                },
            )
            .unwrap();
        vm.set_entry_point(vm.heap.compile_time_ptr(entry), &[]);
        assert_eq!(exec_string(&mut vm), "true|true|true");
        let paths = named_paths(&vm);
        let args: Vec<_> = vm
            .telemetry
            .as_ref()
            .unwrap()
            .span_records()
            .iter()
            .filter_map(|event| match event {
                SpanRecord::FunctionSpanAnnouncement {
                    call_path,
                    captured_type_args: Some(args),
                    ..
                } => Some((paths[call_path].1.clone(), format!("{args:?}"))),
                _ => None,
            })
            .collect();
        assert_eq!(args.len(), 3);
        runs.push(args);
    }
    assert_eq!(runs[0], runs[1]);
}

pub(crate) fn test_int(value: Value) -> i64 {
    value.as_int().expect("an int")
}

fn object(vm: &BexVm, index: &AtomicU32) -> HeapPtr {
    vm.idx_to_ptr(ObjectIndex::from_raw(index.load(Ordering::Relaxed) as usize))
}

fn test_body(body: CompiledFn, slots: u32) -> bex_vm_types::types::CompiledBody {
    crate::compiled_rt::CompiledEntry {
        object: 0,
        name: "",
        arity: 0,
        body,
        slots,
        cause_slot: None,
        framed: false,
    }
    .erase()
}

#[cfg(not(target_arch = "wasm32"))]
fn vm_with_main(body: CompiledFn, slots: u32, telemetry: bool) -> BexVm {
    let mut program = baml_test_support::compile_source(r#"function Main() -> string { "" }"#);
    let entry = program.rendered_callables()["user.Main"].object;
    let Object::Function(function) = &mut program.objects[entry] else {
        unreachable!()
    };
    function.compiled_body = Some(test_body(body, slots));
    let mut vm = BexVm::from_program(program, Arc::new(AtomicBool::new(false))).unwrap();
    if !telemetry {
        vm.telemetry = None;
    }
    vm.set_entry_point(vm.heap.compile_time_ptr(entry.raw()), &[]);
    vm
}

/// Run `user.Main` with compiled bodies installed and publish named object indices.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn run_with_compiled_bodies(
    source: &str,
    bodies: &[(&str, CompiledFn)],
    objects: &[(&str, &AtomicU32)],
    telemetry: bool,
) -> String {
    let level = telemetry.then_some(btel_settings::mode::AutoTelemetryLevel::Medium);
    exec_with_compiled_bodies(source, bodies, objects, level).0
}

#[cfg(not(target_arch = "wasm32"))]
fn assert_compiled_output(
    source: &str,
    bodies: &[(&str, CompiledFn)],
    objects: &[(&str, &AtomicU32)],
    expected: &str,
) {
    for telemetry in [false, true] {
        let out = run_with_compiled_bodies(source, bodies, objects, telemetry);
        assert_eq!(out, expected, "telemetry: {telemetry}");
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn compiled_program(
    source: &str,
    bodies: &[(&str, CompiledFn)],
    objects: &[(&str, &AtomicU32)],
) -> bex_vm_types::Program {
    let mut program = baml_test_support::compile_source(source);
    let callables = program.rendered_callables();
    let resolve = |name: &str| {
        if let Some(callable) = callables.get(name) {
            return callable.object.raw();
        }
        if let Some(index) = (0..program.objects.len()).find(|&index| {
            program
                .function_name(ObjectIndex::from_raw(index))
                .as_deref()
                == Some(name)
        }) {
            return index;
        }
        program
            .packages
            .iter()
            .find_map(|package| {
                package
                    .classes
                    .iter()
                    .chain(&package.interfaces)
                    .find_map(|(path, object)| {
                        let qualified = std::iter::once(package.name.as_str())
                            .chain(path.namespace.iter().map(baml_type::Name::as_str))
                            .chain(std::iter::once(path.name.as_str()))
                            .collect::<Vec<_>>()
                            .join(".");
                        (qualified == name).then(|| object.raw())
                    })
            })
            .unwrap_or_else(|| panic!("no callable, class or interface `{name}`"))
    };
    for (name, slot) in objects {
        slot.store(u32::try_from(resolve(name)).unwrap(), Ordering::Relaxed);
    }
    let indices: Vec<_> = bodies.iter().map(|(name, _)| resolve(name)).collect();
    for ((_, body), index) in bodies.iter().zip(indices) {
        let Object::Function(function) = &mut program.objects[ObjectIndex::from_raw(index)] else {
            unreachable!()
        };
        assert!(matches!(function.kind, FunctionKind::Bytecode));
        // Generated bodies reserve at least their arguments' slots.
        let slots = u32::try_from(function.arity).unwrap();
        function.compiled_body = Some(test_body(*body, slots));
    }
    program
}

#[cfg(not(target_arch = "wasm32"))]
fn exec_with_compiled_bodies(
    source: &str,
    bodies: &[(&str, CompiledFn)],
    objects: &[(&str, &AtomicU32)],
    level: Option<btel_settings::mode::AutoTelemetryLevel>,
) -> (String, BexVm) {
    let program = compiled_program(source, bodies, objects);
    let mut vm = program_vm(program, level);
    let out = exec_string(&mut vm);
    (out, vm)
}

fn program_vm(
    program: bex_vm_types::Program,
    level: Option<btel_settings::mode::AutoTelemetryLevel>,
) -> BexVm {
    let entry = program.rendered_callables()["user.Main"].object.raw();
    let mut vm = BexVm::from_program(program, Arc::new(AtomicBool::new(false))).unwrap();
    vm.telemetry = level.map(|level| {
        TelemetryState::new_root(
            Arc::new(TelemetryPolicies::with_auto_level(level)),
            btel_clock::ClockRuntime::new(btel_clock::ClockMode::Monotonic).start_run(),
            crate::telemetry::test_runtime(),
        )
    });
    vm.set_entry_point(vm.heap.compile_time_ptr(entry), &[]);
    vm
}

#[cfg(not(target_arch = "wasm32"))]
fn exec_string(vm: &mut BexVm) -> String {
    loop {
        match vm.exec() {
            Ok(VmExecState::EarlyYield) => {}
            // The engine answers a log with a null.
            Ok(VmExecState::Log { .. }) => vm.stack.push(Value::NULL),
            // Tests that spawn ignore the task; the body gets a null future.
            Ok(VmExecState::Spawn { .. }) => vm.stack.push(Value::NULL),
            Ok(VmExecState::Complete(value)) => {
                break vm.as_string(&value).unwrap().to_string();
            }
            Ok(other) => panic!("unexpected execution state: {other:?}"),
            Err(error) => break format!("error: {error:?}"),
        }
    }
}

// SAFETY: callers own the heap's only mutator and keep its fiber parked.
#[allow(unsafe_code)]
fn collect(vm: &mut BexVm) -> HashMap<HeapPtr, HeapPtr> {
    let mut roots = Vec::new();
    vm.collect_roots(&mut roots);
    let (_, _, forwarding) = unsafe {
        vm.heap
            .collect_garbage_generational(&roots, CollectionLevel::Major)
    };
    vm.forward_roots(&forwarding);
    forwarding
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
#[allow(unsafe_code)]
fn a_body_paused_at_a_yield_survives_a_moving_collection() {
    use crate::compiled_rt::Step;

    fn body(vm: &mut BexVm, fr: &FrameRecord, _: &[Value]) -> BodyResult {
        vm.reserve_slots(fr, 1);
        let mut live = vm.string_value("still live");
        let mut cached = vm.static_type_value(fr, 0, bex_vm_types::RealizedTy::unknown());
        for _ in 0..3 {
            let before = live;
            vm.set_slot(fr, 0, live);
            vm.suspend(Step::Yield)?;
            live = vm.slot(fr, 0);
            let forwarded = vm.static_type_value(fr, 0, bex_vm_types::RealizedTy::unknown());
            assert_ne!(
                forwarded, cached,
                "the collection forwards the compiled cache"
            );
            assert_eq!(
                forwarded,
                vm.static_type_value(fr, 0, bex_vm_types::RealizedTy::unknown()),
                "the forwarded value remains cached"
            );
            cached = forwarded;
            assert_ne!(live, before, "the collection moves the value");
            assert_eq!(vm.as_string(&live).unwrap().as_str(), "still live");
        }
        Ok(live)
    }
    let mut vm = vm_with_main(body, 1, true);
    let out = loop {
        match vm.exec().unwrap() {
            VmExecState::EarlyYield => {
                collect(&mut vm);
            }
            VmExecState::Complete(value) => break value,
            other => panic!("unexpected execution state: {other:?}"),
        }
    };
    assert_eq!(vm.as_string(&out).unwrap().as_str(), "still live");
}

#[cfg(not(any(target_arch = "wasm32", all(windows, target_arch = "aarch64"))))]
#[test]
fn a_panicking_compiled_body_clears_fiber_state() {
    fn body(_: &mut BexVm, _: &FrameRecord, _: &[Value]) -> BodyResult {
        panic!("compiled body panic");
    }
    let mut vm = vm_with_main(body, 0, true);
    let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| vm.exec()));
    assert!(panic.is_err());
    assert!(!vm.compiled.on_fiber);
    assert!(vm.compiled.yielder.is_none());
    assert!(vm.compiled.fiber.is_none());
}

// Large frames must exhaust fiber space before the VM's frame limit, including
// re-entry through bytecode and when the host thread has a small stack.
#[cfg(not(target_arch = "wasm32"))]
#[test]
fn compiled_recursion_checks_fiber_stack_space() {
    use std::sync::atomic::AtomicUsize;

    use crate::compiled_rt::Step;

    static CALLEE: AtomicU32 = AtomicU32::new(0);
    static DEEPEST: AtomicUsize = AtomicUsize::new(0);
    fn deep<const BYTECODE: bool>(vm: &mut BexVm, fr: &FrameRecord, args: &[Value]) -> BodyResult {
        let n = test_int(args[0]);
        DEEPEST.fetch_max(vm.level_count(), Ordering::Relaxed);
        if n == 0 {
            return Ok(Value::int(0));
        }
        let mut pad = [0u64; 8192];
        let slot = usize::try_from(n).unwrap() % pad.len();
        pad[slot] = 1;
        std::hint::black_box(&mut pad);
        let callee = object(vm, &CALLEE);
        let arg = Value::int(n - 1);
        let below = if BYTECODE {
            vm.push_arg(arg);
            vm.suspend(Step::Call { callee, argc: 1 })?
        } else {
            vm.call_compiled(fr, callee, deep::<false>, &[], &[arg], 0)?
        };
        let one = std::hint::black_box(&pad)[slot];
        Ok(Value::int(test_int(below) + i64::try_from(one).unwrap()))
    }
    const SOURCE: &str = r#"
        function Deep(n: int) -> int { if (n == 0) { 0 } else { 1 + Deep(n - 1) } }
        function Bounce(n: int) -> int { Deep(n) }
        function Main() -> string {
            let shallow = Deep(10);
            let d = Deep(300) catch (e) { baml.panics.StackOverflow => -7 };
            let huge = Deep(1000000) catch (e) { baml.panics.StackOverflow => -8 };
            `${shallow}|${d}|${huge}`
        }
    "#;
    let run = || {
        for (callee, body) in [
            ("user.Deep", deep::<false> as CompiledFn),
            ("user.Bounce", deep::<true>),
        ] {
            for telemetry in [false, true] {
                DEEPEST.store(0, Ordering::Relaxed);
                assert_eq!(
                    run_with_compiled_bodies(
                        SOURCE,
                        &[("user.Deep", body)],
                        &[(callee, &CALLEE)],
                        telemetry,
                    ),
                    "10|-7|-8",
                    "{callee}, telemetry: {telemetry}"
                );
                assert!((11..crate::vm::MAX_FRAMES).contains(&DEEPEST.load(Ordering::Relaxed)));
            }
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

// The engine pushes a null after every log; a compiled body must discard it.
#[cfg(not(target_arch = "wasm32"))]
#[test]
fn compiled_logs_leave_the_stack_as_they_found_it() {
    use std::sync::Mutex;

    static LENGTHS: Mutex<Vec<usize>> = Mutex::new(Vec::new());
    fn body(vm: &mut BexVm, fr: &FrameRecord, _: &[Value]) -> BodyResult {
        for _ in 0..4 {
            vm.log_event(
                fr,
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

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn a_compiled_caller_stands_at_its_call_instruction_while_the_callee_runs() {
    use std::sync::{Mutex, atomic::Ordering};

    use bex_vm_types::bytecode::Instruction;
    static INNER: AtomicU32 = AtomicU32::new(0);
    static MIDDLE: AtomicU32 = AtomicU32::new(0);
    static MIDDLE_CALL: AtomicU32 = AtomicU32::new(0);
    static OUTER_CALL: AtomicU32 = AtomicU32::new(0);
    static SEEN: Mutex<Vec<(usize, usize)>> = Mutex::new(Vec::new());
    #[allow(clippy::unnecessary_wraps)] // the compiled-body signature
    fn inner(vm: &mut BexVm, _fr: &FrameRecord, _args: &[Value]) -> BodyResult {
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
            let pc = vm.frame_pc(idx, None).unwrap_or(frame.faulting_pc);
            let line = trace
                .iter()
                .find(|frame| frame.function_name == name)
                .map_or(usize::MAX, |frame| frame.error_line);
            seen.push((pc, line));
        }
        Ok(Value::int(1))
    }
    fn middle(vm: &mut BexVm, fr: &FrameRecord, _args: &[Value]) -> BodyResult {
        let callee = object(vm, &INNER);
        vm.call_compiled(
            fr,
            callee,
            inner,
            &[],
            &[],
            MIDDLE_CALL.load(Ordering::Relaxed),
        )
    }
    fn outer(vm: &mut BexVm, fr: &FrameRecord, _args: &[Value]) -> BodyResult {
        let callee = object(vm, &MIDDLE);
        vm.call_compiled(
            fr,
            callee,
            middle,
            &[],
            &[],
            OUTER_CALL.load(Ordering::Relaxed),
        )
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
type NamedPaths = HashMap<btel_types::CallPathId, NamedPath>;
type TimingRecords = Vec<(NamedPath, InvocationOutcome, bool)>;

// Resolve run-specific IDs and discard clock readings before comparison.
/// `records` with each usage record's span id dropped: ids come from a
/// per-thread allocator, so two runs number the same spans differently.
fn without_span_ids(
    (timings, spans): (TimingRecords, Vec<String>),
) -> (TimingRecords, Vec<String>) {
    let spans = spans
        .into_iter()
        .map(
            |span| match (span.find("node: TelemetryId("), span.find("), entries")) {
                (Some(start), Some(end)) => format!("{}{}", &span[..start], &span[end + 3..]),
                _ => span,
            },
        )
        .collect();
    (timings, spans)
}

fn named_paths(vm: &BexVm) -> NamedPaths {
    let telemetry = vm.telemetry.as_ref().unwrap();
    let mut paths = NamedPaths::new();
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
    paths
}

fn timing_records(vm: &BexVm, paths: &NamedPaths) -> TimingRecords {
    let telemetry = vm.telemetry.as_ref().unwrap();
    telemetry
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
        .collect()
}

fn telemetry_records(vm: &BexVm) -> (TimingRecords, Vec<String>) {
    use std::fmt::Write as _;

    let telemetry = vm.telemetry.as_ref().unwrap();
    let paths = named_paths(vm);
    let timings = timing_records(vm, &paths);
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
fn inline_observed_model_invokes_charge_usage_before_completion() {
    use btel_settings::mode::AutoTelemetryLevel;

    use crate::compiled_rt::FORCE_CALL_FUNNEL;

    static INVOKE: AtomicU32 = AtomicU32::new(0);
    #[allow(clippy::unnecessary_wraps)]
    fn invoke(_: &mut BexVm, _: &FrameRecord, args: &[Value]) -> BodyResult {
        Ok(args[0])
    }
    fn caller(vm: &mut BexVm, fr: &FrameRecord, args: &[Value]) -> BodyResult {
        let callee = object(vm, &INVOKE);
        vm.call_compiled(fr, callee, invoke, &[], args, 0)
    }

    const SOURCE: &str = r#"
        class Usage {
            input_tokens int
            output_tokens int
        }
        class Model {
            provider string
            model string
        }
        class Call {
            usage Usage
            resolved_model Model
        }
        class Turn { calls Call[] }
        function Invoke(turn: Turn) -> Turn { turn }
        function Caller(turn: Turn) -> Turn { Invoke(turn) }
        function Main() -> string {
            let turn = Turn {
                calls: [Call {
                    usage: Usage { input_tokens: 17, output_tokens: 5 },
                    resolved_model: Model { provider: "test", model: "model" }
                }]
            };
            `${Caller(turn).calls.length()}`
        }
    "#;

    for level in [AutoTelemetryLevel::Medium, AutoTelemetryLevel::High] {
        let mut runs = Vec::new();
        for funnel in [true, false] {
            let mut program = compiled_program(
                SOURCE,
                &[("user.Invoke", invoke), ("user.Caller", caller)],
                &[("user.Invoke", &INVOKE)],
            );
            let invoke_index = program.rendered_callables()["user.Invoke"].object;
            let Object::Function(function) = &mut program.objects[invoke_index] else {
                unreachable!()
            };
            function.body_meta = Some(bex_vm_types::FunctionMeta::ModelInvoke);
            let mut vm = program_vm(program, Some(level));
            FORCE_CALL_FUNNEL.with(|force| force.set(funnel));
            assert_eq!(exec_string(&mut vm), "1");
            FORCE_CALL_FUNNEL.with(|force| force.set(false));
            vm.finish_telemetry(InvocationOutcome::Ok);
            let usages: Vec<_> = vm
                .telemetry
                .as_ref()
                .unwrap()
                .span_records()
                .iter()
                .filter_map(|record| {
                    if let SpanRecord::SpanUsage(usage) = record {
                        Some(&usage.entries)
                    } else {
                        None
                    }
                })
                .collect();
            assert_eq!(
                usages,
                vec![&vec![btel_records::ModelUsage {
                    provider: "test".into(),
                    model: Some("model".into()),
                    calls: 1,
                    input_tokens: Some(17),
                    output_tokens: Some(5),
                    cache_read_tokens: None,
                    cache_write_tokens: None,
                    reasoning_tokens: None,
                }]]
            );
            runs.push(without_span_ids(telemetry_records(&vm)));
        }
        assert_eq!(runs[0], runs[1], "level: {level:?}");
    }
}

// As generated code calls a compiled callee: with a frame record when
// the VM allows, else with a VM frame.
fn call(
    vm: &mut BexVm,
    fr: &FrameRecord,
    callee: HeapPtr,
    body: CompiledFn,
    arg: Value,
) -> BodyResult {
    if vm.direct_calls() {
        vm.call_record(
            fr,
            callee,
            0,
            |_: &mut BexVm| [arg],
            |vm, fr| body(vm, fr, &[arg]),
        )
    } else {
        vm.call_compiled(fr, callee, body, &[], &[arg], 0)
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn inline_observed_compiled_calls_record_as_the_call_path_does() {
    use btel_settings::mode::AutoTelemetryLevel;

    use crate::compiled_rt::{FORCE_CALL_FUNNEL, FORCE_VM_FRAMES};

    static LEAF: AtomicU32 = AtomicU32::new(0);
    static MID: AtomicU32 = AtomicU32::new(0);
    static REC: AtomicU32 = AtomicU32::new(0);
    static DEEP: AtomicU32 = AtomicU32::new(0);
    static PANIC: AtomicU32 = AtomicU32::new(0);
    fn leaf(vm: &mut BexVm, _fr: &FrameRecord, args: &[Value]) -> BodyResult {
        let n = test_int(args[0]);
        if n < 0 {
            return Ok(Value::int(vm.div_i64(n, 0)?));
        }
        Ok(Value::int(n + 1))
    }
    fn mid(vm: &mut BexVm, fr: &FrameRecord, args: &[Value]) -> BodyResult {
        let leaf_ptr = object(vm, &LEAF);
        let a = call(vm, fr, leaf_ptr, leaf, args[0])?;
        let b = call(vm, fr, leaf_ptr, leaf, args[0])?;
        Ok(Value::int(test_int(a) + test_int(b)))
    }
    fn rec(vm: &mut BexVm, fr: &FrameRecord, args: &[Value]) -> BodyResult {
        let n = test_int(args[0]);
        if n == 0 {
            return Ok(Value::int(0));
        }
        let rec_ptr = object(vm, &REC);
        let below = call(vm, fr, rec_ptr, rec, Value::int(n - 1))?;
        Ok(Value::int(test_int(below) + 1))
    }
    fn deep(vm: &mut BexVm, fr: &FrameRecord, args: &[Value]) -> BodyResult {
        let n = test_int(args[0]);
        if n == 0 {
            return Ok(Value::int(vm.div_i64(1, 0)?));
        }
        let deep_ptr = object(vm, &DEEP);
        let below = call(vm, fr, deep_ptr, deep, Value::int(n - 1))?;
        Ok(Value::int(test_int(below) + 1))
    }
    fn guarded(
        vm: &mut BexVm,
        fr: &FrameRecord,
        callee: &AtomicU32,
        body: CompiledFn,
        arg: Value,
        fallback: i64,
    ) -> BodyResult {
        let ptr = object(vm, callee);
        match call(vm, fr, ptr, body, arg) {
            Ok(v) => Ok(v),
            Err(e) => {
                let (thrown, ctx) = vm.catch(e, fr)?;
                let class = object(vm, &PANIC);
                if vm.is_instance_of(thrown, class) {
                    Ok(Value::int(fallback))
                } else {
                    Err(crate::compiled_rt::rethrow(thrown, ctx))
                }
            }
        }
    }
    fn safe(vm: &mut BexVm, fr: &FrameRecord, args: &[Value]) -> BodyResult {
        guarded(vm, fr, &MID, mid, args[0], -1)
    }
    fn guard(vm: &mut BexVm, fr: &FrameRecord, args: &[Value]) -> BodyResult {
        guarded(vm, fr, &DEEP, deep, args[0], -2)
    }
    const SOURCE: &str = r#"
        function Leaf(n: int) -> int { if (n < 0) { n / 0 } else { n + 1 } }
        function Mid(n: int) -> int { Leaf(n) + Leaf(n) }
        function Rec(n: int) -> int { if (n == 0) { 0 } else { 1 + Rec(n - 1) } }
        function Deep(n: int) -> int { if (n == 0) { 1 / 0 } else { 1 + Deep(n - 1) } }
        function Safe(n: int) -> int { Mid(n) catch (e) { baml.panics.DivisionByZero => -1 } }
        function Guard(n: int) -> int { Deep(n) catch (e) { baml.panics.DivisionByZero => -2 } }
        function Main() -> string { `${Safe(1)}|${Safe(-1)}|${Rec(3)}|${Guard(2)}` }
    "#;

    // The VM's call path is the reference; compiled callees entered inline
    // with a VM frame and with a frame record must record the same.
    let modes = [
        ("call path", true, false),
        ("vm frames", false, true),
        ("records", false, false),
    ];
    for level in [AutoTelemetryLevel::Medium, AutoTelemetryLevel::High] {
        let mut runs = Vec::new();
        for (mode, funnel, frames) in modes {
            FORCE_CALL_FUNNEL.with(|force| force.set(funnel));
            FORCE_VM_FRAMES.with(|force| force.set(frames));
            let (out, vm) = exec_with_compiled_bodies(
                SOURCE,
                &[
                    ("user.Leaf", leaf),
                    ("user.Mid", mid),
                    ("user.Rec", rec),
                    ("user.Deep", deep),
                    ("user.Safe", safe),
                    ("user.Guard", guard),
                ],
                &[
                    ("user.Leaf", &LEAF),
                    ("user.Mid", &MID),
                    ("user.Rec", &REC),
                    ("user.Deep", &DEEP),
                    ("baml.panics.DivisionByZero", &PANIC),
                ],
                Some(level),
            );
            FORCE_CALL_FUNNEL.with(|force| force.set(false));
            FORCE_VM_FRAMES.with(|force| force.set(false));
            assert_eq!(out, "4|-1|3|-2", "level {level:?}, {mode}");
            runs.push(telemetry_records(&vm));
        }
        let funnel = &runs[0];
        for ((mode, _, _), run) in modes.iter().zip(&runs).skip(1) {
            assert_eq!(run.0, funnel.0, "timing records at {level:?}, {mode}");
            assert_eq!(run.1, funnel.1, "span records at {level:?}, {mode}");
        }
        let inline = &runs[2];

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
            ("user.Guard", "user.Deep", errored, true),
            ("user.Guard", "user.Deep", errored, true),
            ("user.Guard", "user.Deep", errored, false),
            ("user.Main", "user.Guard", ok, false),
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
                assert_eq!(completions("Panicked"), 5);
                assert_eq!(completions("Reentry"), 5);
            }
            AutoTelemetryLevel::Low => unreachable!(),
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn a_spawn_below_a_frame_record_is_attributed_to_its_body() {
    use btel_settings::mode::AutoTelemetryLevel;
    use btel_types::CallPathEdge;

    use crate::compiled_rt::{FORCE_CALL_FUNNEL, FORCE_VM_FRAMES, Step};

    static INNER: AtomicU32 = AtomicU32::new(0);
    // Spawns the plan it is passed, below `Outer`.
    fn inner(vm: &mut BexVm, fr: &FrameRecord, args: &[Value]) -> BodyResult {
        fr.note_pc(0);
        vm.suspend(Step::Spawn(args[0]))?;
        Ok(Value::int(1))
    }
    fn outer(vm: &mut BexVm, fr: &FrameRecord, args: &[Value]) -> BodyResult {
        let callee = object(vm, &INNER);
        let below = call(vm, fr, callee, inner, args[0])?;
        Ok(Value::int(test_int(below) + 1))
    }
    const SOURCE: &str = r#"
        function Work() -> int { 1 }
        function Inner(p: baml.spawn.Plan<int, never>) -> int { 1 }
        function Outer(p: baml.spawn.Plan<int, never>) -> int { Inner(p) + 1 }
        function Main() -> string {
            let p = baml.spawn.Plan.new(() -> int { Work() });
            `${Outer(p)}`
        }
    "#;

    let modes = [
        ("call path", true, false),
        ("vm frames", false, true),
        ("records", false, false),
    ];
    for level in [AutoTelemetryLevel::Medium, AutoTelemetryLevel::High] {
        let mut runs = Vec::new();
        for (mode, funnel, frames) in modes {
            FORCE_CALL_FUNNEL.with(|force| force.set(funnel));
            FORCE_VM_FRAMES.with(|force| force.set(frames));
            let (out, vm) = exec_with_compiled_bodies(
                SOURCE,
                &[("user.Inner", inner), ("user.Outer", outer)],
                &[("user.Inner", &INNER)],
                Some(level),
            );
            FORCE_CALL_FUNNEL.with(|force| force.set(false));
            FORCE_VM_FRAMES.with(|force| force.set(false));
            assert_eq!(out, "2", "level {level:?}, {mode}");
            // The spawn's call path: the spawned lambda and its visible caller.
            let telemetry = vm.telemetry.as_ref().unwrap();
            let spawned: Vec<Option<String>> = telemetry
                .span_records()
                .iter()
                .filter_map(|event| match event {
                    SpanRecord::CallPathDefined {
                        visible_caller,
                        edge: CallPathEdge::Spawn,
                        ..
                    } => Some(
                        visible_caller
                            .map(|id| vm.heap.function_metadata(vm.proof(), id).unwrap().fqn),
                    ),
                    _ => None,
                })
                .collect();
            assert_eq!(
                spawned,
                [Some("user.Inner".to_string())],
                "spawn caller at {level:?}, {mode}"
            );
            runs.push(telemetry_records(&vm));
        }
        for ((mode, _, _), run) in modes.iter().zip(&runs).skip(1) {
            assert_eq!(run.1, runs[0].1, "span records at {level:?}, {mode}");
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn compiled_bodies_require_the_original_program() {
    use crate::compiled_rt::CompiledEntry;

    fn body(_vm: &mut BexVm, _fr: &FrameRecord, _args: &[Value]) -> BodyResult {
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
        framed: false,
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
    if !super::STACKFUL_FIBERS_SUPPORTED {
        let error = crate::vm::convert_program_with_compiled_bodies(decoded, true, Some(bodies))
            .expect_err("unsupported targets must refuse compiled bodies before execution");
        assert!(error.to_string().contains("require stackful fibers"));

        let empty = Box::leak(Box::new(super::CompiledBodyTable {
            fingerprint: digest,
            entries: &[],
        }));
        let bodies = super::CompiledBodies {
            table: empty,
            artifact_fingerprint: digest,
        };
        crate::vm::convert_program_with_compiled_bodies(program, true, Some(bodies))
            .expect("empty tables remain usable on unsupported targets");
        return;
    }
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
    fn main(vm: &mut BexVm, _fr: &FrameRecord, _args: &[Value]) -> BodyResult {
        RUNS.fetch_add(1, Ordering::Relaxed);
        Ok(vm.string_value("compiled"))
    }
    fn failing_main(vm: &mut BexVm, _fr: &FrameRecord, _args: &[Value]) -> BodyResult {
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
    use crate::compiled_rt::Step;
    static HELPER: AtomicU32 = AtomicU32::new(0);

    fn twice(vm: &mut BexVm, fr: &FrameRecord, args: &[Value]) -> BodyResult {
        let n = test_int(args[0]);
        let helper = object(vm, &HELPER);
        vm.push_arg(Value::int(n));
        match vm.suspend(Step::Call {
            callee: helper,
            argc: 1,
        }) {
            Ok(h) => Ok(Value::int(n * 10 + test_int(h))),
            Err(error) if n == 0 => {
                vm.catch(error, fr)?;
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
    assert_compiled_output(
        SOURCE,
        &[("user.Twice", twice)],
        &[("user.Helper", &HELPER)],
        "70|-1|-9",
    );
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn compiled_delivery_survives_collection_before_resume() {
    use crate::compiled_rt::Step;

    fn body(vm: &mut BexVm, fr: &FrameRecord, _args: &[Value]) -> BodyResult {
        let error = vm
            .suspend(Step::Yield)
            .expect_err("expected a delivered exception");
        let (value, context) = vm.catch(error, fr)?;
        let Object::Instance(context) = vm.get_object(context.as_object_ptr().unwrap()) else {
            panic!("expected error context");
        };
        assert_eq!(context.load_field(0), value);
        assert_eq!(vm.as_string(&value).unwrap(), "delivered error");
        Ok(value)
    }

    let mut vm = vm_with_main(body, 0, false);
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

    let forwarding = collect(&mut vm);
    let moved_value = forwarding[&thrown_ptr];
    let moved_context = forwarding[&context_ptr];
    assert_ne!(moved_value, thrown_ptr);
    assert_ne!(moved_context, context_ptr);
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
fn cancellation_unwinds_records_above_a_function_value_frame() {
    use btel_settings::mode::AutoTelemetryLevel;

    use crate::{
        compiled_rt::{FrameAt, Step},
        errors::VmPanic,
    };

    static LEAF: AtomicU32 = AtomicU32::new(0);
    static SYSOP: AtomicU32 = AtomicU32::new(0);
    fn leaf(vm: &mut BexVm, fr: &FrameRecord, _: &[Value]) -> BodyResult {
        fr.note_pc(0);
        vm.suspend(Step::Call {
            callee: object(vm, &SYSOP),
            argc: 0,
        })
    }
    fn work(vm: &mut BexVm, fr: &FrameRecord, _: &[Value]) -> BodyResult {
        vm.call_record(
            fr,
            object(vm, &LEAF),
            0,
            |_: &mut BexVm| [Value::NULL; 0],
            |vm, fr| leaf(vm, fr, &[]),
        )
    }
    const SOURCE: &str = r#"
        function Leaf() -> null { baml.sys.collect_garbage() }
        function Work() -> null { Leaf() }
        function Invoke(body: () -> null throws never) -> null { body() }
        function Main() -> null { Invoke(Work) }
    "#;

    for level in [
        None,
        Some(AutoTelemetryLevel::Medium),
        Some(AutoTelemetryLevel::High),
    ] {
        let mut records = Vec::new();
        for compiled in [false, true] {
            let bodies: &[(&str, CompiledFn)] = if compiled {
                &[("user.Work", work), ("user.Leaf", leaf)]
            } else {
                &[]
            };
            let program = compiled_program(
                SOURCE,
                bodies,
                &[("user.Leaf", &LEAF), ("baml.sys.collect_garbage", &SYSOP)],
            );
            let mut vm = program_vm(program, level);
            loop {
                match vm.exec().unwrap() {
                    VmExecState::EarlyYield => {}
                    VmExecState::SysOp { .. } => break,
                    other => panic!("expected a sys-op suspension, got {other:?}"),
                }
            }
            if compiled {
                assert!(vm.frames.iter().any(|frame| matches!(
                    vm.get_object(frame.function()),
                    Object::GenericFunction(_)
                )));
            }
            let cancelled = vm.panic_to_exception_value(VmPanic::Cancelled);
            let result = vm.try_handle_external_exception(cancelled);
            let error = if compiled {
                result.expect("cancellation must be delivered to the parked fiber");
                vm.exec().unwrap_err()
            } else {
                result.unwrap_err()
            };
            assert!(matches!(error, VmError::ThrownUnhandled { .. }));
            assert!(
                vm.stack_view()
                    .iter()
                    .all(|at| matches!(at, FrameAt::Vm(_)))
            );
            vm.finish_telemetry(InvocationOutcome::Cancelled);
            if level.is_some() {
                records.push(without_span_ids(telemetry_records(&vm)));
            }
        }
        if level.is_some() {
            assert_eq!(records[0], records[1], "telemetry at {level:?}");
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn compiled_bodies_call_bound_method_values() {
    use crate::compiled_rt::Step;

    fn apply(vm: &mut BexVm, _fr: &FrameRecord, args: &[Value]) -> BodyResult {
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
    assert_compiled_output(SOURCE, &[("user.Apply", apply)], &[], "15|6");
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn global_callee_errors_match_bytecode_at_every_opt_level() {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use baml_test_support::OptLevel;
    use bex_vm_types::{DeclPath, GlobalIndex};

    static SLOT: AtomicUsize = AtomicUsize::new(0);
    static CALL_PC: AtomicUsize = AtomicUsize::new(0);

    fn main(vm: &mut BexVm, fr: &FrameRecord, _: &[Value]) -> BodyResult {
        let value = vm.load_global_checked(fr.function(), SLOT.load(Ordering::Relaxed))?;
        fr.note_pc(u32::try_from(CALL_PC.load(Ordering::Relaxed)).unwrap());
        let callee = vm.indirect_callee(value)?;
        vm.suspend(super::Step::Call { callee, argc: 0 })
    }

    const SOURCE: &str = r#"
const CALLBACK = () -> int { 42 };
function Main() -> int { CALLBACK() }
"#;
    for opt in [OptLevel::Zero, OptLevel::One, OptLevel::Two] {
        let program = baml_test_support::compile_source_with_opt(SOURCE, opt);
        let package = &program.packages[program.root as usize];
        let slot = package
            .globals
            .iter()
            .find_map(|(path, cell)| {
                matches!(path, DeclPath::Let(name) if name.name.as_str() == "CALLBACK")
                    .then_some(package.slot_base.raw() + *cell as usize)
            })
            .unwrap();
        SLOT.store(slot, Ordering::Relaxed);
        let entry = program.rendered_callables()["user.Main"].object;
        let Object::Function(function) = &program.objects[entry] else {
            unreachable!()
        };
        CALL_PC.store(
            function
                .bytecode
                .instructions
                .iter()
                .position(|instruction| {
                    matches!(instruction, bex_vm_types::Instruction::CallIndirect)
                })
                .unwrap(),
            Ordering::Relaxed,
        );
        let globals = program.globals.len();
        for case in 0..5 {
            let mut errors = Vec::new();
            for compiled in [false, true] {
                let mut program = program.clone();
                let Object::Function(function) = &mut program.objects[entry] else {
                    unreachable!()
                };
                if compiled {
                    function.compiled_body = Some(test_body(main, 0));
                }
                let mut vm =
                    BexVm::from_program(program, Arc::new(AtomicBool::new(false))).unwrap();
                vm.telemetry = None;
                let value = match case {
                    0 => Value::int(7),
                    1 => Value::NULL,
                    2 => Value::bool(false),
                    3 => vm.string_value("not callable"),
                    _ => Value::NULL,
                };
                let function = vm.idx_to_ptr(entry);
                vm.store_global(function, slot, value).unwrap();
                if case == 4 {
                    let mut pending = vec![false; globals];
                    pending[slot] = true;
                    vm.pending_globals = Some(pending.into_boxed_slice());
                    assert!(vm.global_is_pending(GlobalIndex::from_raw(slot)));
                }
                vm.set_entry_point(function, &[]);
                let error = loop {
                    match vm.exec() {
                        Ok(VmExecState::EarlyYield) => {}
                        Err(error) => break error,
                        other => panic!("expected a callee error, got {other:?}"),
                    }
                };
                errors.push(format!("{error:?}"));
            }
            assert_eq!(errors[0], errors[1], "{opt:?}, case {case}");
            let kind = if case == 4 {
                "GlobalReadBeforeInit"
            } else {
                "TypeError"
            };
            assert!(errors[0].contains(kind), "{}", errors[0]);
        }
    }
}

// Compiled frames must never use exception handlers from their replaced bytecode.
#[cfg(not(target_arch = "wasm32"))]
#[test]
fn errors_leave_compiled_frames_past_their_bytecode_handlers() {
    static WORK: AtomicU32 = AtomicU32::new(0);
    fn work(vm: &mut BexVm, _fr: &FrameRecord, args: &[Value]) -> BodyResult {
        let n = test_int(args[0]);
        Ok(Value::int(vm.div_i64(n, 0)?))
    }
    // Safe's bytecode catches this error; its compiled body lets it escape.
    fn safe(vm: &mut BexVm, fr: &FrameRecord, args: &[Value]) -> BodyResult {
        let work_ptr = object(vm, &WORK);
        vm.call_compiled(fr, work_ptr, work, &[], &[args[0]], 0)
    }
    const SOURCE: &str = r#"
        function Work(n: int) -> int { n / 0 }
        function Safe(n: int) -> int { Work(n) catch (e) { baml.panics.DivisionByZero => -1 } }
        function Main() -> string {
            let r = Safe(1) catch (e) { baml.panics.DivisionByZero => -9 };
            `${r}`
        }
    "#;
    assert_compiled_output(
        SOURCE,
        &[("user.Work", work), ("user.Safe", safe)],
        &[("user.Work", &WORK)],
        "-9",
    );
}

// Rethrowing must preserve the original throw context across compiled and bytecode frames.
#[cfg(not(target_arch = "wasm32"))]
#[test]
fn compiled_rethrow_keeps_the_original_context() {
    static WORK: AtomicU32 = AtomicU32::new(0);
    fn work(vm: &mut BexVm, _fr: &FrameRecord, args: &[Value]) -> BodyResult {
        let n = test_int(args[0]);
        Ok(Value::int(vm.div_i64(n, 0)?))
    }
    fn relay(vm: &mut BexVm, fr: &FrameRecord, args: &[Value]) -> BodyResult {
        let work_ptr = object(vm, &WORK);
        match vm.call_compiled(fr, work_ptr, work, &[], &[args[0]], 0) {
            Ok(v) => Ok(v),
            Err(e) => {
                let (thrown, ctx) = vm.catch(e, fr)?;
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
    assert_compiled_output(
        SOURCE,
        &[("user.Work", work), ("user.Relay", relay)],
        &[("user.Work", &WORK)],
        &bytecode,
    );
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
            telemetry_definition: bex_vm_types::DefinitionCell::default(),
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

        let forwarding = collect(&mut vm);
        let moved = forwarding
            .get(&class_ptr)
            .copied()
            .expect("the pending native type argument must survive collection");

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

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn compiled_trace_hooks_record_as_bytecode_does() {
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
    fn traced<const MODE: usize>(vm: &mut BexVm, fr: &FrameRecord, args: &[Value]) -> BodyResult {
        let n = args[0];
        if MODE < 4 {
            vm.builtin_trace_hook(
                fr,
                [
                    TraceHookMode::Hidden,
                    TraceHookMode::Timing,
                    TraceHookMode::Span,
                    TraceHookMode::EmptySpan,
                ][MODE],
            )?;
        } else {
            let settings = vm.compiled_begin_trace_hook(fr, true)?;
            if !settings.is_null() {
                let callee = object(vm, if MODE == 4 { &DECIDE } else { &BOOM });
                vm.push_arg(n);
                vm.push_arg(settings);
                let result = vm.suspend(Step::Call { callee, argc: 2 })?;
                vm.compiled_finish_trace_hook(fr, result)?;
            }
        }
        Ok(Value::int(test_int(n) + i64::try_from(MODE + 1).unwrap()))
    }
    fn mid(vm: &mut BexVm, fr: &FrameRecord, args: &[Value]) -> BodyResult {
        let n = args[0];
        let mut total = 0;
        for (index, body) in [
            (&HIDDEN, traced::<0> as CompiledFn),
            (&TIMING, traced::<1>),
            (&SPAN, traced::<2>),
            (&EMPTY, traced::<3>),
            (&HOOKED, traced::<4>),
            (&FALLBACK, traced::<5>),
        ] {
            let callee = object(vm, index);
            total += test_int(vm.call_compiled(fr, callee, body, &[], &[n], 0)?);
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
    const BODIES: &[(&str, CompiledFn)] = &[
        ("user.Hidden", traced::<0>),
        ("user.Timing", traced::<1>),
        ("user.Span", traced::<2>),
        ("user.Empty", traced::<3>),
        ("user.Hooked", traced::<4>),
        ("user.Fallback", traced::<5>),
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
        // Context IDs are hashes of the captured contents. Check the contents
        // and IDs, including hook patches and restoration on normal/error return.
        let contexts = |vm: &BexVm| {
            let pool = btel_snapshot::SnapshotPool::new(1, btel_snapshot::Limits::default());
            vm.telemetry
                .as_ref()
                .unwrap()
                .span_contexts
                .iter()
                .map(|context| {
                    let id = btel_snapshot::context::capture(context, &pool)
                        .expect("a test context fits")
                        .root_id();
                    (context.clone(), id)
                })
                .collect::<Vec<_>>()
        };
        let bytecode_contexts = contexts(&bytecode);
        let compiled_contexts = contexts(&compiled);
        assert!(
            bytecode_contexts
                .iter()
                .any(|(context, _)| { context.metadata().contains_key("decided") })
        );
        assert_eq!(
            compiled_contexts, bytecode_contexts,
            "contexts at {level:?}"
        );
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

#[cfg(not(any(target_arch = "wasm32", all(windows, target_arch = "aarch64"))))]
mod error_tests;
