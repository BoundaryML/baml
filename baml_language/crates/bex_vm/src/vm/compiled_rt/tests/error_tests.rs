use std::sync::atomic::{AtomicU32, Ordering};

use bex_vm_types::Instruction;

use super::*;
use crate::{
    compiled_rt::{Step, rethrow, throw},
    telemetry::FORCE_ERROR_EVIDENCE,
};

// Compare semantic error records, excluding run-specific IDs and clocks.
fn error_records(vm: &BexVm) -> Vec<String> {
    let mut raises = Vec::new();
    vm.telemetry
        .as_ref()
        .unwrap()
        .span_records()
        .iter()
        .filter_map(|record| match record {
            SpanRecord::ErrorRaised {
                raise_id,
                kind,
                pc,
                frame_count,
                ..
            } => {
                raises.push(*raise_id);
                Some(format!("raise {kind:?} pc={pc:?} frames={frame_count}"))
            }
            SpanRecord::ErrorRaisedAndEnded {
                raise_id,
                kind,
                pc,
                frame_count,
                result,
                unwound_frames,
                ..
            } => {
                raises.push(*raise_id);
                Some(format!(
                    "raise/end {kind:?} pc={pc:?} frames={frame_count} {result:?} unwound={unwound_frames}"
                ))
            }
            SpanRecord::ErrorUnwindEnded {
                result,
                unwound_frames,
                ..
            } => Some(format!("end {result:?} unwound={unwound_frames}")),
            SpanRecord::ErrorRaiseOrigin { origin, .. } => Some(match origin {
                btel_records::RaiseOrigin::Proven {
                    origin,
                    previous,
                    via,
                } => format!(
                    "origin {:?} previous={:?} via={via:?}",
                    raises.iter().position(|id| id == origin),
                    previous.map(|id| raises.iter().position(|r| *r == id))
                ),
                other => format!("origin {other:?}"),
            }),
            SpanRecord::ErrorRaiseStack(stack) => {
                Some(format!("stack frames={}", stack.frames.len()))
            }
            SpanRecord::ErrorRaiseFrameCompleted { .. } => Some("promoted".into()),
            _ => None,
        })
        .collect()
}

#[test]
fn compiled_catches_match_bytecode_error_evidence_and_rethrow_origins() {
    static WORK: AtomicU32 = AtomicU32::new(0);
    static THROW_PC: AtomicU32 = AtomicU32::new(0);
    static CALL_PC: AtomicU32 = AtomicU32::new(0);
    static HANDLER_PC: AtomicU32 = AtomicU32::new(0);
    static RETHROW_PC: AtomicU32 = AtomicU32::new(0);
    static MIXED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

    fn work(_: &mut BexVm, fr: &FrameRecord, _: &[Value]) -> Result<Value, Box<VmError>> {
        fr.note_pc(THROW_PC.load(Ordering::Relaxed));
        Err(throw(Value::int(7)))
    }
    fn relay(vm: &mut BexVm, fr: &FrameRecord, args: &[Value]) -> Result<Value, Box<VmError>> {
        let again = args[0].as_bool().unwrap();
        vm.reserve_slots(fr, 2)?;
        let callee = vm.idx_to_ptr(ObjectIndex::from_raw(WORK.load(Ordering::Relaxed) as usize));
        let pc = CALL_PC.load(Ordering::Relaxed);
        let result = if MIXED.load(Ordering::Relaxed) {
            fr.note_pc(pc);
            vm.suspend(Step::Call { callee, argc: 0 })
        } else if vm.direct_calls() {
            vm.call_record(fr, callee, pc, |_| [], |vm, fr| work(vm, fr, &[]))
        } else {
            vm.call_compiled(fr, callee, work, &[], &[], pc)
        };
        match result {
            Ok(value) => Ok(value),
            Err(error) => {
                vm.reserve_slots(fr, 2)?;
                let (value, context) =
                    vm.catch_at(error, fr, 1, HANDLER_PC.load(Ordering::Relaxed) as usize)?;
                if again {
                    fr.note_pc(RETHROW_PC.load(Ordering::Relaxed));
                    Err(rethrow(value, context))
                } else {
                    Ok(Value::int(0))
                }
            }
        }
    }
    const SOURCE: &str = r#"
        function Work() -> int throws int { throw 7 }
        function Relay(again: bool) -> int throws int {
            Work() catch (e) { int => { if (again) { throw e } else { 0 } } }
        }
        function Main() -> string {
            let a = Relay(false);
            let b = Relay(true) catch (e) { int => 9 };
            `${a}:${b}`
        }
    "#;
    let program = baml_test_support::compile_source(SOURCE);
    let callables = program.rendered_callables();
    let Object::Function(work_function) = &program.objects[callables["user.Work"].object] else {
        unreachable!()
    };
    let Object::Function(relay_function) = &program.objects[callables["user.Relay"].object] else {
        unreachable!()
    };
    THROW_PC.store(
        work_function
            .bytecode
            .instructions
            .iter()
            .position(|i| matches!(i, Instruction::Throw))
            .unwrap()
            .try_into()
            .unwrap(),
        Ordering::Relaxed,
    );
    CALL_PC.store(
        relay_function
            .bytecode
            .instructions
            .iter()
            .position(|i| matches!(i, Instruction::Call { .. }))
            .unwrap()
            .try_into()
            .unwrap(),
        Ordering::Relaxed,
    );
    HANDLER_PC.store(
        relay_function.bytecode.exception_table[0]
            .handler_pc
            .try_into()
            .unwrap(),
        Ordering::Relaxed,
    );
    // Skip the generated rethrow for an unmatched catch before the source arm.
    RETHROW_PC.store(
        relay_function
            .bytecode
            .instructions
            .iter()
            .rposition(|i| matches!(i, Instruction::Rethrow))
            .unwrap()
            .try_into()
            .unwrap(),
        Ordering::Relaxed,
    );

    for level in [
        btel_settings::mode::AutoTelemetryLevel::Low,
        btel_settings::mode::AutoTelemetryLevel::Medium,
        btel_settings::mode::AutoTelemetryLevel::High,
    ] {
        FORCE_ERROR_EVIDENCE.with(|force| force.set(true));
        let (out, reference) = exec_with_compiled_bodies(SOURCE, &[], &[], Some(level));
        assert_eq!(out, "0:9");
        let reference = error_records(&reference);
        assert!(
            reference.iter().any(|r| r.contains("origin Some(1)")),
            "{reference:?}"
        );
        for (mixed, frames) in [(false, false), (false, true), (true, false)] {
            MIXED.store(mixed, Ordering::Relaxed);
            super::super::call::FORCE_VM_FRAMES.with(|force| force.set(frames));
            let mut bodies = vec![("user.Relay", relay as crate::compiled_rt::CompiledFn)];
            if !mixed {
                bodies.push(("user.Work", work));
            }
            let (out, vm) =
                exec_with_compiled_bodies(SOURCE, &bodies, &[("user.Work", &WORK)], Some(level));
            assert_eq!(out, "0:9");
            assert_eq!(
                error_records(&vm),
                reference,
                "{level:?} mixed={mixed} frames={frames}"
            );
        }
        super::super::call::FORCE_VM_FRAMES.with(|force| force.set(false));
        FORCE_ERROR_EVIDENCE.with(|force| force.set(false));
    }
}

#[test]
#[allow(unsafe_code)]
fn compiled_awaits_match_bytecode_error_evidence_and_future_origins() {
    use bex_vm_types::{Future, RealizedTy, types::FutureId};
    use btel_records::{FutureErrorLink, OriginEvidence};
    use btel_settings::mode::AutoTelemetryLevel;
    use btel_types::allocate_telemetry_id;

    use crate::types::ObjectTrait;

    struct EvidencePublisher;
    impl<I, V> btel_processor::Publisher<I, V> for EvidencePublisher {
        const ERROR_EVIDENCE: bool = true;
        fn aggregate(&mut self, _: btel_processor::AggregateDelta) {}
        fn span(&mut self, _: btel_types::TelemetryId, _: &mut btel_records::SpanRecord<I, V>) {}
        fn flush(&mut self) {}
    }

    const SOURCE: &str = r#"
        function Wait(f: baml.future.Future<int, int>) -> int throws int { await f }
        function Main(f: baml.future.Future<int, int>) -> int {
            Wait(f) catch (e) { int => -1, baml.panics.Cancelled => -2 }
        }
    "#;
    fn wait(vm: &mut BexVm, fr: &FrameRecord, args: &[Value]) -> Result<Value, Box<VmError>> {
        // Keep the source await's PC, as generated code does before suspending.
        let function = vm.get_object(fr.function()).as_callable().unwrap();
        let pc = function
            .bytecode
            .instructions
            .iter()
            .position(|i| matches!(i, bex_vm_types::Instruction::Await))
            .unwrap();
        fr.note_pc(pc.try_into().unwrap());
        vm.suspend(Step::Await(args[0]))
    }
    fn run(compiled: bool, cancelled: bool, level: AutoTelemetryLevel) -> Vec<String> {
        let mut program = baml_test_support::compile_source(SOURCE);
        let callables = program.rendered_callables();
        let main = callables["user.Main"].object.raw();
        let target = callables["user.Wait"].object;
        if compiled {
            let Object::Function(function) = &mut program.objects[target] else {
                unreachable!()
            };
            function.compiled_body = Some(test_body(wait, 1));
        }
        let mut vm = BexVm::from_program(program, Arc::new(AtomicBool::new(false))).unwrap();
        let runtime = btel_processor::TelemetryRuntime::with_publisher(EvidencePublisher).unwrap();
        vm.telemetry = Some(TelemetryState::new_root(
            Arc::new(TelemetryPolicies::with_auto_level(level)),
            btel_clock::ClockRuntime::new(btel_clock::ClockMode::Monotonic).start_run(),
            Arc::clone(&runtime),
        ));
        let id = FutureId::from_usize(72491);
        let future = vm.tlab.alloc(Object::Future(Future::pending(
            id,
            RealizedTy::int(),
            RealizedTy::int(),
            tokio_util::sync::CancellationToken::new(),
        )));
        let origin = allocate_telemetry_id();
        vm.telemetry.as_ref().unwrap().link_future_error(
            id.as_usize() as u64,
            FutureErrorLink {
                raise: origin,
                origin: OriginEvidence::Known(origin),
            },
        );
        let Object::Future(value) = vm.get_object(future) else {
            unreachable!()
        };
        if cancelled {
            assert!(value.settle_cancelled());
        } else {
            // This isolated VM owns its heap, and no collection runs during settlement.
            assert!(unsafe { value.settle_error(vm.heap.as_ref(), future, Value::int(7), vec![]) });
        }
        vm.set_entry_point(vm.heap.compile_time_ptr(main), &[Value::object(future)]);
        loop {
            match vm.exec().unwrap() {
                VmExecState::EarlyYield => {}
                VmExecState::Complete(value) => {
                    assert_eq!(value.as_int(), Some(if cancelled { -2 } else { -1 }));
                    break;
                }
                other => panic!("unexpected execution state: {other:?}"),
            }
        }
        let telemetry = vm.telemetry.as_ref().unwrap();
        if !cancelled {
            assert!(
                telemetry
                    .span_records()
                    .iter()
                    .any(|record| matches!(record,
                SpanRecord::ErrorRaiseOrigin { origin: btel_records::RaiseOrigin::Proven {
                    origin: found, previous: Some(previous), via: btel_records::OriginVia::Await,
                }, .. } if *found == origin && *previous == origin)),
                "await must link its producer"
            );
        }
        let records = error_records(&vm);
        drop(vm);
        runtime.finish().unwrap();
        records
    }
    for level in [
        AutoTelemetryLevel::Low,
        AutoTelemetryLevel::Medium,
        AutoTelemetryLevel::High,
    ] {
        for cancelled in [false, true] {
            let reference = run(false, cancelled, level);
            let expected = if cancelled { "AwaitCancelled" } else { "Await" };
            assert!(
                reference.iter().any(|r| r.contains(expected)),
                "{reference:?}"
            );
            assert_eq!(
                run(true, cancelled, level),
                reference,
                "{level:?} cancelled={cancelled}"
            );
        }
    }
}

#[test]
fn compiled_raise_kinds_follow_source_opcodes_instead_of_error_values() {
    static THROW_PC: AtomicU32 = AtomicU32::new(0);
    static CALL_PC: AtomicU32 = AtomicU32::new(0);

    fn explicit(vm: &mut BexVm, fr: &FrameRecord, _: &[Value]) -> BodyResult {
        fr.note_pc(THROW_PC.load(Ordering::Relaxed));
        let value = vm.panic_to_exception_value(crate::errors::VmPanic::AssertionFailed);
        Err(throw(value))
    }
    fn boundary(vm: &mut BexVm, fr: &FrameRecord, _: &[Value]) -> BodyResult {
        fr.note_pc(CALL_PC.load(Ordering::Relaxed));
        let value = vm.panic_to_exception_value(crate::errors::VmPanic::MapKeyNotFound);
        Err(throw(value))
    }
    const SOURCE: &str = r#"
        function Explicit() -> int {
            throw baml.panics.AssertionFailed { message: "assertion failed" }
        }
        function Boundary() -> int {
            let map = { "present": 1 };
            map["missing"]
        }
        function Main() -> string {
            let a = Explicit() catch (e) { baml.panics.AssertionFailed => -1 };
            let b = Boundary() catch (e) { baml.panics.MapKeyNotFound => -2 };
            `${a}:${b}`
        }
    "#;
    let program = baml_test_support::compile_source(SOURCE);
    let callables = program.rendered_callables();
    for (name, target, is_throw) in [
        ("user.Explicit", &THROW_PC, true),
        ("user.Boundary", &CALL_PC, false),
    ] {
        let Object::Function(function) = &program.objects[callables[name].object] else {
            unreachable!()
        };
        let pc = function
            .bytecode
            .instructions
            .iter()
            .rposition(|instruction| {
                if is_throw {
                    matches!(instruction, Instruction::Throw)
                } else {
                    // Earlier calls construct the map; the last one indexes it.
                    matches!(instruction, Instruction::Call { .. })
                }
            })
            .unwrap();
        target.store(pc.try_into().unwrap(), Ordering::Relaxed);
    }
    for level in [
        btel_settings::mode::AutoTelemetryLevel::Low,
        btel_settings::mode::AutoTelemetryLevel::Medium,
        btel_settings::mode::AutoTelemetryLevel::High,
    ] {
        FORCE_ERROR_EVIDENCE.with(|force| force.set(true));
        let (out, reference) = exec_with_compiled_bodies(SOURCE, &[], &[], Some(level));
        assert_eq!(out, "-1:-2");
        let reference = error_records(&reference);
        assert!(
            reference.iter().any(|record| record.contains("Throw")),
            "{reference:?}"
        );
        assert!(
            reference
                .iter()
                .any(|record| record.contains("NativeBoundary")),
            "{reference:?}"
        );
        for frames in [false, true] {
            super::super::call::FORCE_VM_FRAMES.with(|force| force.set(frames));
            let (out, vm) = exec_with_compiled_bodies(
                SOURCE,
                &[("user.Explicit", explicit), ("user.Boundary", boundary)],
                &[],
                Some(level),
            );
            assert_eq!(out, "-1:-2");
            assert_eq!(error_records(&vm), reference, "{level:?} frames={frames}");
        }
        super::super::call::FORCE_VM_FRAMES.with(|force| force.set(false));
        FORCE_ERROR_EVIDENCE.with(|force| force.set(false));
    }
}
