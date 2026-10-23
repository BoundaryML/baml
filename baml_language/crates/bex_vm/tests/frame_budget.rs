//! Rust observes the frame layout, backing capacity, and simultaneous live frames;
//! BAML test blocks cannot inspect these storage invariants.
use std::sync::{Arc, atomic::AtomicBool};

use baml_test_support::compile_source;
use bex_vm::{
    BexVm, VmExecState,
    vm::{BytecodeFrame, FRAME_STORAGE_BUDGET_BYTES, Frame, NativeFrame},
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

fn maximum_slots() -> usize {
    FRAME_STORAGE_BUDGET_BYTES / size_of::<Frame>()
}

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
}

fn drive(vm: &mut BexVm) -> Observation {
    let mut peak = 0;
    let mut capacity = 0;
    let result = loop {
        peak = peak.max(vm.frames.len());
        capacity = capacity.max(vm.frames.capacity());
        // Also stop a regressed unbounded allocator promptly, rather than
        // letting this negative control exhaust the test process's memory.
        assert!(vm.frames.len() <= maximum_slots());
        assert!(vm.frames.capacity() * size_of::<Frame>() <= FRAME_STORAGE_BUDGET_BYTES);
        match vm.exec() {
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
    }
}

#[test]
fn exact_and_general_calls_execute_to_the_layout_derived_budget() {
    let maximum = maximum_slots();
    let depth = i64::try_from(maximum - 2).unwrap();
    for entry in ["user.main", "user.generic_main"] {
        let program = compile_source(SOURCE);
        let mut vm = make_vm(program, entry, depth);
        let Some(Frame::Bytecode(frame)) = vm.frames.first() else {
            panic!("expected entry frame")
        };
        let entry_ptr = frame.function;
        let observation = drive(&mut vm);
        assert_eq!(observation.result, Ok(depth), "{entry}");
        assert_eq!(observation.peak, maximum, "{entry}");
        assert_eq!(observation.capacity, maximum, "{entry}");
        assert!(
            observation.peak > 256,
            "regression control for the old ceiling"
        );

        // Reuse retained capacity at precisely the next depth. Both dispatch
        // paths must refuse the next slot, even with already allocated storage.
        vm.set_entry_point(entry_ptr, &[Value::int(depth + 1)]);
        let observation = drive(&mut vm);
        assert_eq!(
            observation.result,
            Err("panics.StackOverflow".to_string()),
            "{entry}"
        );
        assert_eq!(observation.peak, maximum, "{entry}");
        assert_eq!(observation.capacity, maximum, "{entry}");
    }
}

#[test]
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
        let observation = drive(&mut vm);
        assert_eq!(observation.result, expected, "{entry}");
        assert_eq!(observation.peak, maximum_slots(), "{entry}");
        assert_eq!(observation.capacity, maximum_slots(), "{entry}");
    }
}

#[test]
fn indirect_and_native_callback_frames_share_the_budget() {
    for entry in ["user.indirect_main", "user.native_main"] {
        // These paths use multiple frame records per source-level recursion.
        // Keep a positive case far beyond 256 frames, then exceed the byte
        // budget with a depth based on this build's layout, never a foreign one.
        let success = i64::try_from(maximum_slots() / 8).unwrap();
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
        assert_eq!(observation.capacity, maximum_slots(), "{entry}");
    }
    // Three root-depth offsets force exhaustion at each phase of a native
    // call: the continuation slot, the callback frame, and its recursive callee.
    // Which offset hits which phase depends on this build's Frame layout.
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
        assert_eq!(observation.capacity, maximum_slots(), "{entry}");
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
fn measure_frame_capacity() {
    let depth: i64 = std::env::var("BAML_PROBE_DEPTH").unwrap().parse().unwrap();
    let entry = std::env::var("BAML_PROBE_ENTRY").unwrap_or_else(|_| "user.main".to_string());
    let mut vm = make_vm(compile_source(SOURCE), &entry, depth);
    let observation = drive(&mut vm);
    eprintln!(
        "depth={depth} entry={entry} frame_size={} frame_align={} bytecode_size={} native_size={} peak={} capacity={} capacity_bytes={} outcome={:?}",
        size_of::<Frame>(),
        align_of::<Frame>(),
        size_of::<BytecodeFrame>(),
        size_of::<NativeFrame>(),
        observation.peak,
        observation.capacity,
        observation.capacity * size_of::<Frame>(),
        observation.result
    );
}
