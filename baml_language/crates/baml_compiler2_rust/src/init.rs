//! Native bodies for the emitter's initializer chainers, which have no MIR.

use std::fmt::Write as _;

use bex_vm_types::{
    ConstValue, FunctionKind, Object, Program,
    bytecode::Instruction,
    types::{Function, FunctionIdentity},
};

use crate::{
    admit::{CompiledFunction, Rejection},
    index::ProgramIndex,
};

/// Keep calls on the VM funnel so initialization has bytecode call, error,
/// trace, and telemetry semantics, including calls to uncompiled helpers.
pub(crate) fn compile_chainers(
    program: &Program,
    index: &ProgramIndex,
    bodies: &mut String,
    functions: &mut Vec<CompiledFunction>,
    rejected: &mut Vec<(String, Rejection)>,
) {
    for (object, item) in program.objects.iter().enumerate() {
        let Object::Function(function) = item else {
            continue;
        };
        if !matches!(function.kind, FunctionKind::Bytecode) {
            continue;
        }
        match function.identity {
            FunctionIdentity::Init => {}
            FunctionIdentity::InitLet(_) if function.bytecode.block_pcs.is_empty() => {}
            _ => continue,
        }
        let symbol = format!("init_chain_{object}");
        match generate(program, function, &symbol) {
            Some(source) => {
                bodies.push_str(&source);
                functions.push(CompiledFunction {
                    name: index.name(object).to_owned(),
                    display: index.display(object).to_owned(),
                    symbol,
                    object,
                    arity: function.arity,
                    slots: 0,
                    cause_slot: None,
                    framed: true,
                });
            }
            None => rejected.push((
                index.display(object).to_owned(),
                Rejection::Unsupported("unrecognized initializer bytecode".into()),
            )),
        }
    }
}

fn generate(program: &Program, function: &Function, symbol: &str) -> Option<String> {
    let bytecode = &function.bytecode;
    if function.arity != 0
        || !function.type_param_names.is_empty()
        || function.trace_passthrough
        || function.body_meta.is_some()
        || !bytecode.call_layouts.is_empty()
        || !bytecode.exception_table.is_empty()
        || !bytecode.handler_context_table.is_empty()
        || !bytecode.shield_table.is_empty()
        || !matches!(bytecode.constants.as_slice(), [ConstValue::Null])
    {
        return None;
    }
    let (steps, tail) = bytecode.instructions.split_last_chunk::<2>()?;
    if !matches!(tail, [Instruction::LoadConst(0), Instruction::Return]) {
        return None;
    }
    if matches!(function.identity, FunctionIdentity::InitLet(_)) && !steps.is_empty() {
        return None;
    }
    let mut source = format!(
        "pub fn {symbol}(vm: &mut BexVm, fr: &FrameRecord, args: &[Value]) -> R<Value> {{\n"
    );
    let width = 2;
    if !steps.len().is_multiple_of(width) {
        return None;
    }
    for (step, instructions) in steps.chunks_exact(width).enumerate() {
        let [
            Instruction::Call {
                callee,
                ntypeargs: 0,
            },
            Instruction::StoreGlobal(target),
        ] = instructions
        else {
            return None;
        };
        let call_pc = step * width;
        let call_pc = u32::try_from(call_pc).ok()?;
        let ConstValue::Object(callee_object) = program.globals.get(callee.raw())? else {
            return None;
        };
        let Object::Function(callee_function) = program.objects.get(callee_object.raw())? else {
            return None;
        };
        if callee_function.arity != 0 || target.raw() >= program.globals.len() {
            return None;
        }
        writeln!(source, "fr.note_pc({call_pc});").ok()?;
        writeln!(
            source,
            "let callee = bex_vm::compiled_rt::callee_ptr(vm.load_global(fr.function(), {}))?;",
            callee.raw()
        )
        .ok()?;
        source.push_str("let value = vm.suspend(Step::Call { callee, argc: 0 })?;\n");
        writeln!(source, "fr.note_pc({});", call_pc + 1).ok()?;
        writeln!(
            source,
            "vm.store_global(fr.function(), {}, value)?;",
            target.raw()
        )
        .ok()?;
    }
    let return_pc = u32::try_from(bytecode.instructions.len() - 1).ok()?;
    writeln!(source, "fr.note_pc({return_pc});\nOk(Value::NULL)\n}}\n").ok()?;
    Some(source)
}
