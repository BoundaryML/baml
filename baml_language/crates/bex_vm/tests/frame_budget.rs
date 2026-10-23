//! Rust observes the frame layout, backing capacity, and simultaneous live frames;
//! BAML test blocks cannot inspect these storage invariants.
use std::{
    fmt::Write as _,
    sync::{Arc, atomic::AtomicBool},
};

use baml_test_support::compile_source;
use bex_vm::{
    BexVm, VmExecState,
    vm::{BytecodeFrame, Frame, NativeFrame, STACK_STORAGE_BUDGET_BYTES},
};
use bex_vm_types::{EarlyYieldCheck, Object, Program, Value, errors::VmError};

const SOURCE: &str = r#"
    function depth(n: int) -> int {
        if (n == 0) { 0 } else { 1 + depth(n - 1) }
    }
    function main(n: int) -> int { depth(n) }
    function generic_depth<T>(n: int, value: T) -> int {
        if (n == 0) { 0 } else { 1 + generic_depth<T>(n - 1, value) }
    }
    function generic_main(n: int) -> int { generic_depth<int>(n, 42) }
    function invoke(f: (int) -> int, n: int) -> int { f(n) }
    function indirect_depth(n: int) -> int {
        if (n == 0) { 0 } else { 1 + invoke(indirect_depth, n - 1) }
    }
    function indirect_main(n: int) -> int { indirect_depth(n) }
    function native_depth(n: int) -> int {
        if (n == 0) { 0 } else {
            [n - 1, -1].map((next: int) -> int {
                if (next < 0) { 0 } else { 1 + native_depth(next) }
            })[0]
        }
    }
    function native_main(n: int) -> int { native_depth(n) }
    function native_padded(n: int) -> int { native_main(n) }
    function leaf() -> int { 42 }
    function named(n: int) -> int {
        let result = depth(n) catch (e) { baml.panics.StackOverflow => -1 };
        if (result == -1) { leaf() } else { result }
    }
    function wildcard(n: int) -> int {
        depth(n) catch_all (e) { _ => -2 }
    }
    function panics(n: int) -> int {
        let result = depth(n) catch_all_panics (e) { _ => -3 };
        if (result == -3) { leaf() } else { result }
    }
    function native_named(n: int) -> int {
        let result = native_depth(n) catch (e) { baml.panics.StackOverflow => -1 };
        if (result == -1) { [42].map((value: int) -> int { value })[0] } else { result }
    }
"#;

/// A conservative recursion upper bound, even before accounting for values.
fn maximum_slots() -> usize {
    STACK_STORAGE_BUDGET_BYTES / size_of::<Frame>()
}

/// Create an opcode-yielding VM so storage peaks remain observable.
fn make_vm(program: Program, entry: &str, depth: i64) -> BexVm {
    let index = program.rendered_callables()[entry].object.raw();
    let flag = Arc::new(AtomicBool::new(true));
    let mut vm = BexVm::from_program(program, Arc::clone(&flag)).unwrap();
    // Every-op yields observe live storage without consuming another VM frame.
    vm.early_yield = EarlyYieldCheck::with_interval(flag, 1);
    vm.set_entry_point(vm.heap.compile_time_ptr(index), &[Value::int(depth)]);
    vm
}

#[derive(Debug)]
struct Observation {
    result: Result<i64, String>,
    peak: usize,
    capacity: usize,
    value_peak: usize,
    value_capacity: usize,
    bytes: usize,
}

/// Execute while checking the backing-capacity invariant after every yield.
fn drive(vm: &mut BexVm) -> Observation {
    let mut peak = 0;
    let mut capacity = 0;
    let mut value_peak = 0;
    let mut value_capacity = 0;
    let mut bytes = 0;
    let mut sample = |vm: &BexVm| {
        peak = peak.max(vm.frames.len());
        capacity = capacity.max(vm.frames.capacity());
        value_peak = value_peak.max(vm.stack.len());
        value_capacity = value_capacity.max(vm.stack.capacity());
        let allocated =
            vm.frames.capacity() * size_of::<Frame>() + vm.stack.capacity() * size_of::<Value>();
        bytes = bytes.max(allocated);
        // Also stop a regressed unbounded allocator promptly, rather than
        // letting this negative control exhaust the test process's memory.
        assert!(vm.frames.len() <= maximum_slots());
        assert!(allocated <= STACK_STORAGE_BUDGET_BYTES);
    };
    let result = loop {
        sample(vm);
        let state = vm.exec();
        // Observe capacity retained by a failing instruction too, after the
        // unwinder has removed its live frames/values.
        sample(vm);
        match state {
            Ok(VmExecState::EarlyYield) => {}
            Ok(VmExecState::Complete(value)) => break Ok(value.as_int().unwrap()),
            Err(VmError::ThrownUnhandled { value, .. }) => {
                let Object::Instance(instance) = vm.get_object(value.as_object_ptr().unwrap())
                else {
                    panic!("expected panic instance")
                };
                let Object::Class(class) = vm.get_object(instance.class) else {
                    panic!("expected panic class")
                };
                break Err(class.path.to_string());
            }
            Err(error) => panic!("unexpected execution failure: {error:?}"),
            other => panic!("unexpected: {other:?}"),
        }
    };
    Observation {
        result,
        peak,
        capacity,
        value_peak,
        value_capacity,
        bytes,
    }
}

/// Find a fresh VM's practical depth boundary for this source and layout.
fn boundary(program: &Program, entry: &str) -> i64 {
    let failure = drive(&mut make_vm(
        program.clone(),
        entry,
        i64::try_from(maximum_slots()).unwrap(),
    ));
    assert_eq!(failure.result, Err("panics.StackOverflow".to_string()));
    // These programs have one entry frame plus depth(n)..depth(0). The
    // observed failing run locates the boundary without a layout-specific
    // count or repeatedly replaying a full binary search in ordinary CI.
    let mut candidate = i64::try_from(failure.peak.saturating_sub(2)).unwrap();
    loop {
        let result = drive(&mut make_vm(program.clone(), entry, candidate)).result;
        if result.is_err() {
            assert_eq!(result, Err("panics.StackOverflow".to_string()));
            candidate = candidate
                .checked_sub(1)
                .filter(|&depth| depth >= 0)
                .expect("depth zero must fit");
            continue;
        }
        let next = drive(&mut make_vm(program.clone(), entry, candidate + 1)).result;
        if next.is_err() {
            assert_eq!(next, Err("panics.StackOverflow".to_string()));
            return candidate;
        }
        candidate += 1;
    }
}

#[test]
/// Both dispatchers must respect the combined budget, including retained capacity.
fn exact_and_general_calls_execute_to_the_measured_budget() {
    let program = compile_source(SOURCE);
    for entry in ["user.main", "user.generic_main"] {
        let depth = boundary(&program, entry);
        assert!(depth > 256, "positive control for the old fixed ceiling");
        let mut vm = make_vm(program.clone(), entry, depth);
        let Some(Frame::Bytecode(frame)) = vm.frames.first() else {
            panic!("expected entry frame")
        };
        let entry_ptr = frame.function;
        let observation = drive(&mut vm);
        assert_eq!(observation.result, Ok(depth));
        assert!(observation.value_peak > observation.peak);
        assert!(observation.bytes <= STACK_STORAGE_BUDGET_BYTES);

        vm.set_entry_point(entry_ptr, &[Value::int(depth + 1)]);
        assert_eq!(
            drive(&mut vm).result,
            Err("panics.StackOverflow".to_string())
        );
    }
}

#[test]
/// Real locals must lower practical recursion depth under the same byte budget.
fn larger_frames_consume_more_of_the_shared_budget() {
    let mut source = SOURCE.to_string();
    source.push_str("function wide_depth(n: int) -> int { ");
    for index in 0..64 {
        write!(source, "let local{index} = n + {index}; ").unwrap();
    }
    source.push_str("if (n == 0) { 0 } else { wide_depth(n - 1)");
    for index in 0..64 {
        write!(source, " + local{index}").unwrap();
    }
    source.push_str(" } } function wide_main(n: int) -> int { wide_depth(n) }");
    let program = compile_source(&source);
    let narrow = boundary(&program, "user.main");
    let wide = boundary(&program, "user.wide_main");
    assert!(wide > 256);
    assert!(
        wide < narrow / 2,
        "64 live locals must consume value-stack bytes"
    );
    let success = drive(&mut make_vm(program.clone(), "user.wide_main", wide));
    assert!(success.result.is_ok());
    let failure = drive(&mut make_vm(program, "user.wide_main", wide + 1));
    assert_eq!(failure.result, Err("panics.StackOverflow".to_string()));
}

#[test]
/// Overflow must preserve named, ordinary wildcard, and panic wildcard behavior.
fn the_next_frame_is_a_panic_with_the_existing_catching_boundary() {
    let depth = i64::try_from(maximum_slots() - 1).unwrap();
    for (entry, expected) in [
        ("user.main", Err("panics.StackOverflow".to_string())),
        ("user.generic_main", Err("panics.StackOverflow".to_string())),
        ("user.wildcard", Err("panics.StackOverflow".to_string())),
        ("user.named", Ok(42)),
        ("user.panics", Ok(42)),
    ] {
        let mut vm = make_vm(compile_source(SOURCE), entry, depth);
        let Some(Frame::Bytecode(frame)) = vm.frames.first() else {
            panic!("expected entry frame")
        };
        let entry_ptr = frame.function;
        let observation = drive(&mut vm);
        assert_eq!(observation.result, expected, "{entry}");
        assert!(observation.bytes <= STACK_STORAGE_BUDGET_BYTES);
        if expected.is_ok() {
            assert!(vm.frames.is_empty());
            vm.set_entry_point(entry_ptr, &[Value::int(1000)]);
            assert_eq!(
                drive(&mut vm).result,
                Ok(1000),
                "reuse after caught overflow"
            );
        }
    }
}

#[test]
/// Callback continuations and bytecode calls must consume the same backing budget.
fn indirect_and_native_callback_frames_share_the_budget() {
    for entry in ["user.indirect_main", "user.native_main"] {
        // These paths use multiple frame records per source-level recursion.
        // Keep a positive case far beyond 256 frames, then exceed the byte
        // budget with a depth based on this build's layout, never a foreign one.
        let success = 1000;
        let mut vm = make_vm(compile_source(SOURCE), entry, success);
        let observation = drive(&mut vm);
        assert_eq!(observation.result, Ok(success), "{entry}");
        assert!(observation.peak > 256, "{entry}");

        let failure = i64::try_from(maximum_slots()).unwrap();
        let mut vm = make_vm(compile_source(SOURCE), entry, failure);
        let observation = drive(&mut vm);
        assert_eq!(
            observation.result,
            Err("panics.StackOverflow".to_string()),
            "{entry}"
        );
        assert!(observation.bytes <= STACK_STORAGE_BUDGET_BYTES);
    }
    // Exercise three root-depth offsets around native callback dispatch.
    // Either record or value growth may exhaust the combined budget; the
    // failing phase depends on layout and the two arrays' growth histories.
    for entry in ["user.native_depth", "user.native_padded"] {
        let mut vm = make_vm(
            compile_source(SOURCE),
            entry,
            i64::try_from(maximum_slots()).unwrap(),
        );
        let observation = drive(&mut vm);
        assert_eq!(
            observation.result,
            Err("panics.StackOverflow".to_string()),
            "{entry}"
        );
        assert!(observation.bytes <= STACK_STORAGE_BUDGET_BYTES);
    }
    let mut vm = make_vm(
        compile_source(SOURCE),
        "user.native_named",
        i64::try_from(maximum_slots()).unwrap(),
    );
    assert_eq!(
        drive(&mut vm).result,
        Ok(42),
        "unwinding native continuations frees slots for another callback"
    );
}

#[test]
#[ignore = "manual release-build frame capacity measurement; set BAML_PROBE_DEPTH"]
#[allow(
    clippy::print_stderr,
    reason = "manual measurement probe reports observed layout and capacity"
)]
/// Report observed release layout and capacity for the requested recursion input.
fn measure_frame_capacity() {
    let input = std::env::var("BAML_PROBE_DEPTH").unwrap();
    let entry = std::env::var("BAML_PROBE_ENTRY").unwrap_or_else(|_| "user.main".to_string());
    let mut source = SOURCE.to_string();
    if let Ok(locals) = std::env::var("BAML_PROBE_LOCALS") {
        let locals: usize = locals.parse().unwrap();
        source.push_str("function measured_depth(n: int) -> int { ");
        for index in 0..locals {
            write!(source, "let local{index} = n + {index}; ").unwrap();
        }
        source.push_str("if (n == 0) { 0 } else { 1 + measured_depth(n - 1)");
        for index in 0..locals {
            write!(source, " + local{index}").unwrap();
        }
        source.push_str(" } } function measured_main(n: int) -> int { measured_depth(n) }");
    }
    let program = compile_source(&source);
    let depth = if input == "search" {
        boundary(&program, &entry)
    } else {
        input.parse().unwrap()
    };
    let mut vm = make_vm(program, &entry, depth);
    let observation = drive(&mut vm);
    eprintln!(
        "depth={depth} entry={entry} frame_size={} frame_align={} bytecode_size={} native_size={} peak={} capacity={} frame_bytes={} value_size={} value_peak={} value_capacity={} total_bytes={} outcome={:?}",
        size_of::<Frame>(),
        align_of::<Frame>(),
        size_of::<BytecodeFrame>(),
        size_of::<NativeFrame>(),
        observation.peak,
        observation.capacity,
        observation.capacity * size_of::<Frame>(),
        size_of::<Value>(),
        observation.value_peak,
        observation.value_capacity,
        observation.bytes,
        observation.result
    );
}
