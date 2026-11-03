//! Bytecode disassembly for snapshot tests.

use std::fmt::Write;

use bex_vm_types::{bytecode::Instruction, types::Function};

fn sanitize_operand_text(text: &str) -> String {
    let mut sanitized = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '\n' => sanitized.push_str("\\n"),
            '\r' => sanitized.push_str("\\r"),
            '\t' => sanitized.push_str("\\t"),
            '\0' => sanitized.push_str("\\0"),
            other => sanitized.push(other),
        }
    }
    sanitized
}

/// Display bytecode in a human-readable textual assembly format.
///
/// This format replaces numeric indices with resolved names and uses labels
/// instead of raw jump offsets, producing output that reads like assembly:
///
/// ```text
/// function add(x: int, y: int) -> int {
///     load_var x
///     load_var y
///     bin_op +
///     return
/// }
/// ```
///
/// Labels create new indent blocks (label at 4, code under label at 8):
///
/// ```text
/// function example(x: bool) -> int {
///     load_var x
///     pop_jump_if_false L0
///     load_const 1
///     return
///     L0:
///         load_const 0
///         return
/// }
/// ```
///
/// This format is stable across global index changes (adding builtins won't
/// shift indices) making it suitable for snapshot tests.
fn display_bytecode_textual(function: &Function) -> String {
    use std::collections::{BTreeMap, BTreeSet};

    let instructions = &function.bytecode.instructions;

    if instructions.is_empty() {
        return String::new();
    }

    // --- Pass 1: Collect all jump targets so we can assign labels. ---
    let mut jump_targets = BTreeSet::new();
    // Map from target PC → symbolic name (for jump table arms with named variants).
    let mut target_names: std::collections::HashMap<usize, String> =
        std::collections::HashMap::new();

    for (ip, instruction) in instructions.iter().enumerate() {
        match instruction {
            Instruction::Jump(offset)
            | Instruction::PopJumpIfFalse(offset)
            | Instruction::PopJumpIfTrue(offset)
            | Instruction::JumpIfFalseOrPop(offset)
            | Instruction::JumpIfTrueOrPop(offset)
            | Instruction::JumpIfNotNullOrPop(offset)
            | Instruction::JumpIfFalse(offset) => {
                let target = ip.wrapping_add_signed(*offset);
                jump_targets.insert(target);
            }
            Instruction::JumpTable(table_idx) => {
                // Default target.
                let default_target =
                    ip.wrapping_add_signed(function.bytecode.jump_tables[*table_idx].default);
                jump_targets.insert(default_target);
                // Each entry in the jump table.
                if let Some(table) = function.bytecode.jump_tables.get(*table_idx) {
                    for (i, offset) in table.offsets.iter().enumerate() {
                        if let Some(offset) = offset {
                            let target = ip.wrapping_add_signed(*offset);
                            jump_targets.insert(target);
                            if let Some(name) = table.names.get(i).and_then(|n| n.as_deref()) {
                                target_names.insert(target, name.to_string());
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }

    // Assign labels L0, L1, L2, ... in order of target IP.
    let label_map: BTreeMap<usize, String> = jump_targets
        .iter()
        .enumerate()
        .map(|(i, &ip)| (ip, format!("L{i}")))
        .collect();

    // --- Pass 2: Render each instruction. ---
    // If labels exist: labels at 2-space indent, code at 4-space indent.
    // If no labels: all code at 4-space indent.
    let mut lines: Vec<String> = Vec::with_capacity(instructions.len());

    for (ip, instruction) in instructions.iter().enumerate() {
        let text = display_instruction_textual(ip, instruction, function, &label_map);

        if let Some(label) = label_map.get(&ip) {
            if ip > 0 {
                lines.push(String::new());
            }
            if let Some(name) = target_names.get(&ip) {
                lines.push(format!("  {label}: {name}"));
            } else {
                lines.push(format!("  {label}:"));
            }
        }

        lines.push(format!("    {text}"));
    }

    let mut output = lines.join("\n");
    output.push('\n');
    output
}

/// Render a single instruction in textual format.
///
/// Reads resolved operand names from `InstructionMeta` (populated by the compiler).
/// Jump offsets are resolved to label references.
fn display_instruction_textual(
    ip: usize,
    instruction: &Instruction,
    function: &Function,
    label_map: &std::collections::BTreeMap<usize, String>,
) -> String {
    // Helper: get the operand string from metadata, with a fallback.
    let meta_str = |fallback: &dyn std::fmt::Display| -> String {
        let raw = function
            .bytecode
            .meta
            .get(ip)
            .and_then(|m| m.operand.as_ref())
            .map(|o| o.as_str().to_string())
            .unwrap_or_else(|| format!("?{fallback}"));
        sanitize_operand_text(&raw)
    };

    match instruction {
        // --- Constants ---
        Instruction::LoadConst(_) => format!("load_const {}", meta_str(&"")),

        // --- Variables ---
        Instruction::LoadVar(idx) => format!("load_var {}", meta_str(idx)),
        Instruction::StoreVar(idx) => format!("store_var {}", meta_str(idx)),
        Instruction::StoreVarLoadVar(idx) => format!("store_var_load_var {}", meta_str(idx)),
        Instruction::LoadVar2(a, b) => format!("load_var2 {a} {b}"),
        Instruction::StoreVar2(a, b) => format!("store_var2 {a} {b}"),

        // --- Globals ---
        Instruction::LoadGlobal(idx) => format!("load_global {}", meta_str(&idx.raw())),
        Instruction::StoreGlobal(idx) => format!("store_global {}", meta_str(&idx.raw())),

        // --- Fields ---
        Instruction::LoadField(idx) => {
            let name = meta_str(idx);
            format!("load_field .{name}")
        }
        Instruction::StoreField(idx) => {
            let name = meta_str(idx);
            format!("store_field .{name}")
        }
        // The operand indexes the *interface's* field list, not the receiver's
        // layout, so show the name and mark it virtual.
        Instruction::VirtualLoadField(idx) => {
            let name = meta_str(idx);
            format!("virtual_load_field .{name}")
        }
        Instruction::VirtualStoreField(idx) => {
            let name = meta_str(idx);
            format!("virtual_store_field .{name}")
        }
        Instruction::InitField(idx) => {
            let name = meta_str(idx);
            format!("init_field .{name}")
        }
        Instruction::InitSpread(idx) => format!("init_spread {}", meta_str(idx)),

        // --- Stack ---
        Instruction::Pop(n) => format!("pop {n}"),
        Instruction::Copy(i) => format!("copy {i}"),

        // --- Jumps (resolved to labels) ---
        Instruction::Jump(offset) => {
            let target = ip.wrapping_add_signed(*offset);
            let label = label_map
                .get(&target)
                .cloned()
                .unwrap_or_else(|| format!("?{target}"));
            format!("jump {label}")
        }
        Instruction::PopJumpIfFalse(offset) => {
            let target = ip.wrapping_add_signed(*offset);
            let label = label_map
                .get(&target)
                .cloned()
                .unwrap_or_else(|| format!("?{target}"));
            format!("pop_jump_if_false {label}")
        }
        Instruction::JumpIfFalse(offset) => {
            let target = ip.wrapping_add_signed(*offset);
            let label = label_map
                .get(&target)
                .cloned()
                .unwrap_or_else(|| format!("?{target}"));
            format!("jump_if_false {label}")
        }
        Instruction::PopJumpIfTrue(offset)
        | Instruction::JumpIfFalseOrPop(offset)
        | Instruction::JumpIfTrueOrPop(offset)
        | Instruction::JumpIfNotNullOrPop(offset) => {
            let name = match instruction {
                Instruction::PopJumpIfTrue(_) => "pop_jump_if_true",
                Instruction::JumpIfFalseOrPop(_) => "jump_if_false_or_pop",
                Instruction::JumpIfTrueOrPop(_) => "jump_if_true_or_pop",
                _ => "jump_if_not_null_or_pop",
            };
            let target = ip.wrapping_add_signed(*offset);
            let label = label_map
                .get(&target)
                .cloned()
                .unwrap_or_else(|| format!("?{target}"));
            format!("{name} {label}")
        }
        Instruction::JumpTable(table_idx) => {
            let default_target =
                ip.wrapping_add_signed(function.bytecode.jump_tables[*table_idx].default);
            let default_label = label_map
                .get(&default_target)
                .cloned()
                .unwrap_or_else(|| format!("?{default_target}"));

            let mut entries = Vec::new();
            if let Some(table) = function.bytecode.jump_tables.get(*table_idx) {
                for entry in &table.offsets {
                    match entry {
                        Some(offset) => {
                            let target = ip.wrapping_add_signed(*offset);
                            let label = label_map
                                .get(&target)
                                .cloned()
                                .unwrap_or_else(|| format!("?{target}"));
                            entries.push(label);
                        }
                        None => entries.push("_".to_string()),
                    }
                }
            }

            if entries.is_empty() {
                format!("jump_table default {default_label}")
            } else {
                format!(
                    "jump_table [{}], default {default_label}",
                    entries.join(", ")
                )
            }
        }

        Instruction::Throw => "throw".to_string(),
        Instruction::Rethrow => "rethrow".to_string(),

        // --- Operators ---
        Instruction::BinOp(op) => format!("bin_op {op}"),
        Instruction::CmpOp(op) => format!("cmp_op {op}"),
        Instruction::AddInt => "add_int".to_string(),
        Instruction::SubInt => "sub_int".to_string(),
        Instruction::MulInt => "mul_int".to_string(),
        Instruction::DivInt => "div_int".to_string(),
        Instruction::ModInt => "mod_int".to_string(),
        Instruction::AddFloat => "add_float".to_string(),
        Instruction::SubFloat => "sub_float".to_string(),
        Instruction::MulFloat => "mul_float".to_string(),
        Instruction::DivFloat => "div_float".to_string(),
        Instruction::AddBigint => "add_bigint".to_string(),
        Instruction::SubBigint => "sub_bigint".to_string(),
        Instruction::MulBigint => "mul_bigint".to_string(),
        Instruction::DivBigint => "div_bigint".to_string(),
        Instruction::ModBigint => "mod_bigint".to_string(),
        Instruction::BitAndBigint => "bit_and_bigint".to_string(),
        Instruction::BitOrBigint => "bit_or_bigint".to_string(),
        Instruction::BitXorBigint => "bit_xor_bigint".to_string(),
        Instruction::ShlBigint => "shl_bigint".to_string(),
        Instruction::ShrBigint => "shr_bigint".to_string(),
        Instruction::CmpIntOp(op) => format!("cmp_int_op {op}"),
        Instruction::CmpFloatOp(op) => format!("cmp_float_op {op}"),
        Instruction::CmpBigintOp(op) => format!("cmp_bigint_op {op}"),
        Instruction::UnaryOp(op) => format!("unary_op {op}"),

        // --- Allocation ---
        Instruction::AllocArray(n) => format!("alloc_array {n}"),
        Instruction::ConcatStrings(n) => format!("concat_strings {n}"),
        Instruction::AllocMap(n) => format!("alloc_map {n}"),
        Instruction::AllocInstance {
            class_obj: _,
            ntypeargs,
        } => {
            let name = meta_str(&"");
            if *ntypeargs > 0 {
                format!("alloc_instance<{ntypeargs}> {name}")
            } else {
                format!("alloc_instance {name}")
            }
        }
        Instruction::InitInstance(plan_idx) => {
            let name = meta_str(&"");
            let ntypeargs = function
                .bytecode
                .class_init_plans
                .get(*plan_idx)
                .map_or(0, |plan| plan.ntypeargs);
            if ntypeargs > 0 {
                format!("init_instance<{ntypeargs}> {name}")
            } else {
                format!("init_instance {name}")
            }
        }
        Instruction::AllocVariant(_) => format!("alloc_variant {}", meta_str(&"")),

        // --- Array/Map element access ---
        Instruction::LoadArrayElement => "load_array_element".to_string(),
        Instruction::LoadMapElement => "load_map_element".to_string(),
        Instruction::StoreArrayElement => "store_array_element".to_string(),
        Instruction::StoreMapElement => "store_map_element".to_string(),

        // --- Calls ---
        Instruction::Call { .. } => format!("call {}", meta_str(&"")),
        Instruction::CallHooked { .. } => format!("call_hooked {}", meta_str(&"")),
        Instruction::BeginTraceHook(value) => format!("begin_trace_hook {value}"),
        Instruction::EndTraceHook => "end_trace_hook".into(),
        Instruction::TraceHookHidden => "trace_hook_hidden".into(),
        Instruction::TraceHookTiming => "trace_hook_timing".into(),
        Instruction::TraceHookSpan => "trace_hook_span".into(),
        Instruction::TraceHookRich => "trace_hook_rich".into(),
        Instruction::TraceHookEmptySpan => "trace_hook_empty_span".into(),
        Instruction::SatisfiesBound => "satisfies_bound".into(),

        Instruction::CallIndirect => "call_indirect".to_string(),
        Instruction::SetCallTrace => "set_call_trace".to_string(),

        Instruction::VirtualCall {
            nargs,
            ntypeargs,
            self_arg,
        } => {
            format!("virtual_call nargs={nargs} ntypeargs={ntypeargs} self_arg={self_arg}")
        }

        Instruction::SysOp(_) => format!("sys_op {}", meta_str(&"")),

        Instruction::Spawn => "spawn".to_string(),
        Instruction::Await => "await".to_string(),
        Instruction::AwaitAny => "await_any".to_string(),

        // --- Control ---
        Instruction::Return => "return".to_string(),
        Instruction::Unreachable => "unreachable".to_string(),

        // --- Type introspection ---
        Instruction::Discriminant => "discriminant".to_string(),
        Instruction::TypeTag => "type_tag".to_string(),
        Instruction::IsType(const_idx) => {
            let name = meta_str(const_idx);
            format!("is_type {name}")
        }
        Instruction::NarrowBind { ty, destination } => {
            let name = meta_str(ty);
            format!("narrow_bind {name}, slot={destination}")
        }
        Instruction::LoadType(const_idx) => {
            let name = meta_str(const_idx);
            format!("load_type {name}")
        }
        Instruction::BindType(slot) => format!("bind_type {slot}"),
        Instruction::LoadCurrentPackage(const_idx) => {
            let name = meta_str(const_idx);
            format!("load_current_package {name}")
        }
        Instruction::DenseTag(table_idx) => {
            let names = function
                .bytecode
                .switch_tables
                .get(*table_idx)
                .map(|t| t.key_names.join(", "))
                .unwrap_or_default();
            format!("dense_tag [{names}]")
        }

        // --- Closures and cells ---
        Instruction::MakeClosure {
            obj_idx,
            capture_count,
            ntypeargs,
        } => {
            let name = meta_str(&obj_idx.raw());
            if *ntypeargs > 0 {
                format!("make_closure {name}, captures={capture_count}, ntypeargs={ntypeargs}")
            } else {
                format!("make_closure {name}, {capture_count}")
            }
        }
        Instruction::MakeBoundMethod(_) => {
            let name = meta_str(&"");
            format!("make_bound_method {name}")
        }
        Instruction::MakeVirtualBoundMethod { ntypeargs } => {
            format!("make_virtual_bound_method ntypeargs={ntypeargs}")
        }
        Instruction::MakeVirtualFunction { ntypeargs } => {
            format!("make_virtual_function ntypeargs={ntypeargs}")
        }
        Instruction::MakeGenericFunction { ntypeargs, .. } => {
            let name = meta_str(&"");
            format!("make_generic_function {name} ntypeargs={ntypeargs}")
        }
        Instruction::MakeGenericBoundMethod { ntypeargs, .. } => {
            let name = meta_str(&"");
            format!("make_generic_bound_method {name} ntypeargs={ntypeargs}")
        }
        Instruction::MakeGenericFunctionFromValue { ntypeargs } => {
            format!("make_generic_function_from_value ntypeargs={ntypeargs}")
        }
        Instruction::MakeCell => "make_cell".to_string(),
        Instruction::LoadDeref(slot) => {
            let name = meta_str(slot);
            format!("load_deref {name}")
        }
        Instruction::StoreDeref(slot) => {
            let name = meta_str(slot);
            format!("store_deref {name}")
        }
        Instruction::LoadCapture(idx) => format!("load_capture {idx}"),
        Instruction::StoreCapture(idx) => format!("store_capture {idx}"),
        Instruction::CaptureRef(idx) => format!("capture_ref {idx}"),
        Instruction::SendEvent => "send_event".to_string(),
        Instruction::ContainerLen => "container_len".to_string(),
    }
}

/// Display a full program as human-readable assembly with labels:
///
/// ```text
/// function add(x: int, y: int) -> int {
///     load_var x
///     load_var y
///     bin_op +
///     return
/// }
/// ```
pub fn display_program(functions: &[(String, &Function)]) -> String {
    let mut output = String::new();

    for (i, (name, func)) in functions.iter().enumerate() {
        if i > 0 {
            output.push('\n');
        }

        // Function header with typed parameter names and return type.
        let params: String = func
            .param_names
            .iter()
            .zip(func.param_types.iter())
            .map(|(name, ty)| format!("{name}: {ty}"))
            .collect::<Vec<_>>()
            .join(", ");
        let _ = writeln!(
            output,
            "function {name}({params}) -> {} {{",
            func.return_type
        );

        output.push_str(&display_bytecode_textual(func));
        output.push_str("}\n");
    }

    output
}
