//! Select exact bytecode calls once, after builtin attachment and linking.

use std::collections::HashSet;

use bex_vm_types::{ConstValue, Instruction, Object, bytecode::OpCode, types::FunctionKind};

pub(crate) fn specialize_calls(objects: &mut [Object], globals: &[ConstValue]) {
    // Even init-time writes must not invalidate a specialization. This also
    // keeps hand-authored programs that replace a function global on `Call`.
    let written: HashSet<_> = objects
        .iter()
        .filter_map(|object| match object {
            Object::Function(function) => Some(&function.bytecode.instructions),
            _ => None,
        })
        .flatten()
        .filter_map(|instruction| match instruction {
            Instruction::StoreGlobal(global) => Some(global.raw()),
            _ => None,
        })
        .collect();

    // Inspect the whole immutable image before modifying any compact streams.
    let mut sites = Vec::new();
    for (index, object) in objects.iter().enumerate() {
        let Object::Function(function) = object else {
            continue;
        };
        let compact = function
            .bytecode
            .compact
            .as_ref()
            .expect("lowered bytecode");
        let mut pc = 0;
        let mut inside_hook_prologue = false;
        while pc < compact.code.len() {
            let op = OpCode::try_from(compact.code[pc]).expect("encoded opcode");
            match op {
                OpCode::BeginTraceHook => inside_hook_prologue = true,
                OpCode::EndTraceHook => inside_hook_prologue = false,
                _ => {}
            }
            // The hook invocation uses transparent caller resolution. Keep it
            // on the general path without adding checks to ordinary exact calls.
            if op == OpCode::Call && !inside_hook_prologue {
                let global = u32::from_le_bytes(compact.code[pc + 1..pc + 5].try_into().unwrap());
                let ntypeargs =
                    u16::from_le_bytes(compact.code[pc + 5..pc + 7].try_into().unwrap());
                if !written.contains(&(global as usize))
                    && let Some(ConstValue::Object(callee_index)) = globals.get(global as usize)
                    && let Some(Object::Function(callee)) = objects.get(callee_index.raw())
                    && matches!(callee.kind, FunctionKind::Bytecode)
                {
                    // External package call sites do not have source declaration
                    // metadata. Classify them once from the linked callee, before
                    // any execution; hooked entries must never use the exact path.
                    if callee
                        .bytecode
                        .compact
                        .as_ref()
                        .is_some_and(|code| code.trace_hook_finish_pc.is_some())
                    {
                        sites.push((index, pc, OpCode::CallHooked));
                        pc += op.encoded_size();
                        continue;
                    }
                    if ntypeargs == 0
                        && callee.generic_param_bounds.is_empty()
                        && callee.display_type_params.is_empty()
                        && compact
                            .call_layouts
                            .get(&pc)
                            .is_none_or(|layout| callee.argument_layout_is(layout))
                    {
                        sites.push((index, pc, OpCode::CallExactArgs));
                    }
                }
            }
            pc += op.encoded_size();
        }
    }
    for (index, pc, op) in sites {
        let Object::Function(function) = &mut objects[index] else {
            unreachable!("site belongs to a function");
        };
        function.bytecode.compact.as_mut().unwrap().code[pc] = op as u8;
        // Retain the layout for the general fallback if a pending type lane
        // requires it. Exact calls never search this map during execution.
    }
}
