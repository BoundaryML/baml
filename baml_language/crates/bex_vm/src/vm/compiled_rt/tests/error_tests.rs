use std::sync::atomic::{AtomicU32, Ordering};

use bex_vm_types::Instruction;

use super::*;
use crate::{
    compiled_rt::{Step, rethrow, throw},
    telemetry::FORCE_ERROR_EVIDENCE,
};

// Compare semantic error records, excluding run-specific IDs, clocks and PCs.
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
                frame_count,
                ..
            } => {
                raises.push(*raise_id);
                Some(format!("raise {kind:?} frames={frame_count}"))
            }
            SpanRecord::ErrorRaisedAndEnded {
                raise_id,
                kind,
                frame_count,
                result,
                unwound_frames,
                ..
            } => {
                raises.push(*raise_id);
                Some(format!(
                    "raise/end {kind:?} frames={frame_count} {result:?} unwound={unwound_frames}"
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
        vm.reserve_slots(fr, 2);
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
                vm.reserve_slots(fr, 2);
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
    RETHROW_PC.store(
        relay_function
            .bytecode
            .instructions
            .iter()
            .position(|i| matches!(i, Instruction::Rethrow))
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
