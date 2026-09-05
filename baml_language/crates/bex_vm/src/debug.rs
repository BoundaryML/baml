//! VM debugging utilities & helpers.
//!
// Debug display uses unsafe pointer dereferencing for HeapPtr
#![allow(unsafe_code)]
//!
//! NOTE: Functions here should not take an entire reference to the
//! [`crate::BexVm`] because then it will be hard to circumvent the borrow checker
//! in the [`crate::BexVm::exec`] loop (which is where we want to use this).
//!
//! Instead, they take read only references to the parts of the [`crate::BexVm`]
//! that they need, that way inside the loop we can "destructure" the
//! [`crate::BexVm`] and let the compiler know exactly which properties we're
//! using as mutable and which properties we're using as immutable.
//!
//! Reference structs can be created if needed:
//!
//! ```ignore
//! struct InstructionContext<'vm> {
//!     instruction_ptr: isize,
//!     function: &'vm Function,
//!     stack: &'vm [Value],
//!     objects: &'vm [Object],
//!     globals: &'vm [Value],
//! }
//!
//! ```

use std::fmt::Write;

use bex_vm_types::{
    ConstValue, HeapPtr,
    bytecode::{CompactCode, Instruction, OpCode, read_i8, read_i32, read_u16, read_u32},
    indexable::{GlobalIndex, ObjectPool},
    types::{Function, Object, Value},
};
use console::Style;

/// Display format for bytecode output.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BytecodeFormat {
    /// Human-readable assembly-like format with labels and resolved names.
    /// This is the default used for snapshot tests.
    #[default]
    Textual,
    /// Expanded raw format showing source lines, bytecode addresses,
    /// raw operand indices, and metadata annotations in a table layout.
    Expanded,
}

/// Resolve a global reference (global slot or callee slot) to display metadata.
fn display_global_ref(
    index: GlobalIndex,
    globals: &[Value],
    objects: Option<&ObjectPool>,
    compile_time_globals: Option<&[bex_vm_types::ConstValue]>,
) -> String {
    // Prefer runtime globals.
    if index.raw() < globals.len() {
        return format!("({})", display_value(globals[index.raw()]));
    }

    // At compile time, resolve from compile-time globals/object pool.
    if let (Some(ct_globals), Some(objs)) = (compile_time_globals, objects)
        && let Some(const_val) = ct_globals.get(index.raw())
    {
        return format!("({})", display_const_value(const_val, Some(objs)));
    }

    format!("(global {})", index.raw())
}

/// Return the source line column text for a bytecode PC.
///
/// Returns an empty string when line info is unavailable or unchanged.
fn display_source_line_cell(function: &Function, pc: usize, last_line: &mut usize) -> String {
    match function.bytecode.source_line_for_pc(pc) {
        line if line != 0 && line != *last_line => {
            *last_line = line;
            line.to_string()
        }
        _ => String::new(),
    }
}

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

/// Context aware instruction display.
///
/// Instructions themselves are kinda "bare". For example, `LOAD_VAR 1`
/// means load the local variable at index 1, but what's the name of that
/// variable in the user's code? Same with `LOAD_CONST 1`, what's the value
/// of the constant at index 1?
///
/// This function returns a tuple `(instruction, metadata)` where `metadata`
/// is important debug information associated with the `instruction`. In
/// the case of `LOAD_VAR` it's the name of the variable, in the case of
/// `LOAD_CONST` it's the value of the constant, and so on.
///
/// If there's no relevant metadata to attach to the instruction, then this
/// function returns an empty string.
pub(crate) fn display_instruction(
    instruction_ptr: usize,
    function: &Function,
    globals: &[Value],
    objects: Option<&ObjectPool>,
    compile_time_globals: Option<&[bex_vm_types::ConstValue]>,
) -> (String, String) {
    let instruction = &function.bytecode.instructions[instruction_ptr];
    let operand_meta = function
        .bytecode
        .meta
        .get(instruction_ptr)
        .and_then(|m| m.operand.as_ref());

    let metadata = match instruction {
        Instruction::LoadConst(index) | Instruction::LoadCurrentPackage(index) => {
            // Prefer resolved_constants (runtime), fall back to constants (compile-time)
            if let Some(value) = function.bytecode.resolved_constants.get(*index) {
                format!("({})", display_value(*value))
            } else if let Some(const_value) = function.bytecode.constants.get(*index) {
                format!("({})", display_const_value(const_value, objects))
            } else {
                format!("(const {index})")
            }
        }
        Instruction::LoadGlobal(index) | Instruction::StoreGlobal(index) => {
            display_global_ref(*index, globals, objects, compile_time_globals)
        }
        Instruction::Call { callee, .. }
        | Instruction::CallWithRuntimeId { callee, .. }
        | Instruction::SysOp(callee)
        | Instruction::SysOpWithRuntimeId(callee) => {
            display_global_ref(*callee, globals, objects, compile_time_globals)
        }
        Instruction::MakeGenericFunction { function, .. } => {
            display_global_ref(*function, globals, objects, compile_time_globals)
        }
        Instruction::MakeGenericFunctionFromValue { .. } => String::new(),
        Instruction::VirtualCall { nargs, ntypeargs }
        | Instruction::VirtualCallWithRuntimeId { nargs, ntypeargs } => {
            format!("nargs={nargs} ntypeargs={ntypeargs}")
        }
        Instruction::LoadVar(index)
        | Instruction::StoreVar(index)
        | Instruction::StoreVarLoadVar(index) => match function.local_names.get(*index) {
            Some(name) => format!("({name})"),
            None => "(?)".to_string(),
        },
        Instruction::LoadField(_)
        | Instruction::VirtualLoadField(_)
        | Instruction::VirtualStoreField(_)
        | Instruction::StoreField(_)
        | Instruction::InitField(_)
        | Instruction::InitSpread(_)
        | Instruction::InitInstance(_) => operand_meta
            .map(|m| format!("({})", m.as_str()))
            .unwrap_or_default(),
        Instruction::NarrowBind { ty, destination } => {
            let destination = function
                .local_names
                .get(*destination)
                .map_or("?", String::as_str);
            let narrowed_type =
                operand_meta.map_or_else(|| format!("?{ty}"), |meta| meta.as_str().to_owned());
            format!("(type {narrowed_type}, {destination})")
        }
        Instruction::Jump(offset)
        | Instruction::PopJumpIfFalse(offset)
        | Instruction::PopJumpIfTrue(offset)
        | Instruction::JumpIfFalseOrPop(offset)
        | Instruction::JumpIfTrueOrPop(offset)
        | Instruction::JumpIfNotNullOrPop(offset)
        | Instruction::JumpIfFalse(offset) => {
            format!("(to {})", instruction_ptr.wrapping_add_signed(*offset))
        }
        Instruction::AllocInstance {
            class_obj: index, ..
        }
        | Instruction::AllocVariant(index) => {
            // Look up the class/enum from the compile-time ObjectPool if available
            if let Some(objs) = objects {
                format!("({})", display_object_from_pool(index.raw(), objs))
            } else {
                "(object)".to_string()
            }
        }

        Instruction::JumpTable(table_idx) => {
            format!("(table {table_idx})")
        }
        Instruction::DenseTag(table_idx) => {
            if let Some(table) = function.bytecode.match_hash_tables.get(*table_idx) {
                format!("({})", table.key_names.join(", "))
            } else {
                format!("(hash table {table_idx})")
            }
        }
        Instruction::Pop(_)
        | Instruction::Copy(_)
        | Instruction::BinOp(_)
        | Instruction::CmpOp(_)
        | Instruction::AddInt
        | Instruction::SubInt
        | Instruction::MulInt
        | Instruction::DivInt
        | Instruction::ModInt
        | Instruction::AddFloat
        | Instruction::SubFloat
        | Instruction::MulFloat
        | Instruction::DivFloat
        | Instruction::AddBigint
        | Instruction::SubBigint
        | Instruction::MulBigint
        | Instruction::DivBigint
        | Instruction::ModBigint
        | Instruction::BitAndBigint
        | Instruction::BitOrBigint
        | Instruction::BitXorBigint
        | Instruction::ShlBigint
        | Instruction::ShrBigint
        | Instruction::CmpIntOp(_)
        | Instruction::CmpFloatOp(_)
        | Instruction::CmpBigintOp(_)
        | Instruction::LoadVar2(..)
        | Instruction::StoreVar2(..)
        | Instruction::UnaryOp(_)
        | Instruction::AllocArray(_)
        | Instruction::AllocMap(_)
        | Instruction::LoadArrayElement
        | Instruction::LoadMapElement
        | Instruction::StoreArrayElement
        | Instruction::StoreMapElement
        | Instruction::Await
        | Instruction::AwaitAny
        | Instruction::CallIndirect
        | Instruction::CallIndirectWithRuntimeId
        | Instruction::Throw
        | Instruction::Rethrow
        | Instruction::Discriminant
        | Instruction::TypeTag
        | Instruction::RuntimeIsType
        | Instruction::IsType(_)
        | Instruction::ThrowIfPanic
        | Instruction::Unreachable
        | Instruction::MakeClosure { .. }
        | Instruction::MakeBoundMethod(_)
        | Instruction::MakeVirtualBoundMethod { .. }
        | Instruction::MakeVirtualFunction { .. }
        | Instruction::MakeCell
        | Instruction::LoadDeref(_)
        | Instruction::StoreDeref(_)
        | Instruction::LoadCapture(_)
        | Instruction::StoreCapture(_)
        | Instruction::CaptureRef(_)
        | Instruction::Return
        | Instruction::SendEvent
        | Instruction::ContainerLen
        | Instruction::Spawn
        | Instruction::LoadType(_)
        | Instruction::BindType(_) => String::new(),
    };

    (instruction.to_string(), metadata)
}

/// Context aware value display.
///
/// The default display for objects is just a reference number. If we want
/// all the information, we have to dereference the object and call it's
/// `to_string` implementation.
pub(crate) fn display_value(value: Value) -> String {
    match value.as_object_ptr() {
        Some(ptr) => display_object_ptr(ptr),
        None => value.to_string(),
    }
}

/// Display a compile-time constant value.
///
/// At compile time, we only have `ConstValue` (with `ObjectIndex` for objects).
/// If `ObjectPool` is provided, we can resolve object indices to actual values.
fn display_const_value(value: &bex_vm_types::ConstValue, objects: Option<&ObjectPool>) -> String {
    match value {
        bex_vm_types::ConstValue::OmittedArg => "<omitted>".to_string(),
        bex_vm_types::ConstValue::Null => "null".to_string(),
        bex_vm_types::ConstValue::Int(i) => i.to_string(),
        bex_vm_types::ConstValue::Float(f) => bex_vm_types::format_float(*f),
        bex_vm_types::ConstValue::Bool(b) => b.to_string(),
        bex_vm_types::ConstValue::Object(idx) => {
            if let Some(objs) = objects {
                display_object_from_pool(idx.raw(), objs)
            } else {
                format!("<object {}>", idx.raw())
            }
        }
        bex_vm_types::ConstValue::Type(template) => format!("<type_template {template}>"),
        bex_vm_types::ConstValue::Literal(literal) => format!("<literal {literal}>"),
        bex_vm_types::ConstValue::ClassWithTypeArgs {
            class_obj,
            type_args_templates,
        } => {
            format!(
                "<class_with_type_args obj={} args={type_args_templates:?}>",
                class_obj.raw()
            )
        }
    }
}

/// Display an object from the compile-time `ObjectPool`.
fn display_object_from_pool(index: usize, objects: &ObjectPool) -> String {
    if let Some(obj) = objects.get(index) {
        match obj {
            Object::String(s) => format!("{s:?}"),
            Object::Function(f) => format!("<fn {}>", f.name),
            Object::Class(c) => format!("<class {}>", c.name),
            Object::Enum(e) => format!("<enum {}>", e.name),
            _ => format!("<object {index}>"),
        }
    } else {
        format!("<object {index}>")
    }
}

fn display_object_ptr(ptr: HeapPtr) -> String {
    // SAFETY: During debug display, we assume the pointer is valid
    let object = unsafe { ptr.get() };
    match object {
        Object::String(string) => format!("{string:?}"),
        // This one's a bit tricky to print.
        Object::Instance(instance) => {
            // SAFETY: During debug display, we assume the pointer is valid
            let class = unsafe { instance.class.get() };
            match class {
                Object::Class(class) => format!("<{} instance>", class.name),
                // This will most likely never happen, but we're trying not
                // to panic.
                other => format!("<{other} instance>"),
            }
        }

        Object::Variant(variant) => {
            // SAFETY: During debug display, we assume the pointer is valid
            let enm = unsafe { variant.enm.get() };
            match enm {
                Object::Enum(enm) => format!("<{} variant>", enm.name),
                other => format!("<{other} variant>"),
            }
        }

        other => other.to_string(),
    }
}

/// The number of whitespaces that separate each column.
///
/// See [`display_bytecode`] for more information.
const COLUMN_MARGIN: usize = 3;

/// Get color style for an instruction based on its type.
fn instruction_style(instruction: &Instruction) -> Style {
    match instruction {
        Instruction::LoadConst(_)
        | Instruction::LoadCurrentPackage(_)
        | Instruction::LoadVar(_)
        | Instruction::LoadVar2(..)
        | Instruction::LoadGlobal(_)
        | Instruction::LoadField(_)
        | Instruction::VirtualLoadField(_)
        | Instruction::LoadArrayElement
        | Instruction::LoadMapElement => Style::new().blue(),
        Instruction::StoreVar(_)
        | Instruction::StoreVarLoadVar(_)
        | Instruction::StoreVar2(..)
        | Instruction::StoreGlobal(_)
        | Instruction::StoreField(_)
        | Instruction::VirtualStoreField(_)
        | Instruction::InitField(_)
        | Instruction::InitSpread(_)
        | Instruction::StoreArrayElement
        | Instruction::StoreMapElement => Style::new().green(),
        Instruction::BinOp(_)
        | Instruction::CmpOp(_)
        | Instruction::AddInt
        | Instruction::SubInt
        | Instruction::MulInt
        | Instruction::DivInt
        | Instruction::ModInt
        | Instruction::AddFloat
        | Instruction::SubFloat
        | Instruction::MulFloat
        | Instruction::DivFloat
        | Instruction::AddBigint
        | Instruction::SubBigint
        | Instruction::MulBigint
        | Instruction::DivBigint
        | Instruction::ModBigint
        | Instruction::BitAndBigint
        | Instruction::BitOrBigint
        | Instruction::BitXorBigint
        | Instruction::ShlBigint
        | Instruction::ShrBigint
        | Instruction::CmpIntOp(_)
        | Instruction::CmpFloatOp(_)
        | Instruction::CmpBigintOp(_)
        | Instruction::UnaryOp(_) => Style::new().blue().bright(),
        Instruction::Jump(_)
        | Instruction::PopJumpIfFalse(_)
        | Instruction::PopJumpIfTrue(_)
        | Instruction::JumpIfFalseOrPop(_)
        | Instruction::JumpIfTrueOrPop(_)
        | Instruction::JumpIfNotNullOrPop(_)
        | Instruction::JumpIfFalse(_)
        | Instruction::JumpTable { .. }
        | Instruction::DenseTag(_) => Style::new().yellow(),
        Instruction::Call { .. }
        | Instruction::CallWithRuntimeId { .. }
        | Instruction::CallIndirect
        | Instruction::CallIndirectWithRuntimeId
        | Instruction::VirtualCall { .. }
        | Instruction::VirtualCallWithRuntimeId { .. } => Style::new().magenta(),
        Instruction::Return
        | Instruction::Pop(_)
        | Instruction::Copy(_)
        | Instruction::Throw
        | Instruction::Rethrow => Style::new().red(),
        Instruction::AllocMap(_)
        | Instruction::AllocInstance { .. }
        | Instruction::InitInstance(_)
        | Instruction::AllocVariant(_)
        | Instruction::AllocArray(_) => Style::new().cyan(),
        Instruction::SysOp(_)
        | Instruction::SysOpWithRuntimeId(_)
        | Instruction::Spawn
        | Instruction::Await
        | Instruction::AwaitAny => Style::new().green().bright(),
        Instruction::Discriminant
        | Instruction::TypeTag
        | Instruction::RuntimeIsType
        | Instruction::IsType(_)
        | Instruction::NarrowBind { .. }
        | Instruction::LoadType(_)
        | Instruction::BindType(_)
        | Instruction::ThrowIfPanic => Style::new().blue().bright(),
        Instruction::Unreachable => Style::new().red().bright(),
        Instruction::MakeClosure { .. }
        | Instruction::MakeBoundMethod(_)
        | Instruction::MakeVirtualBoundMethod { .. }
        | Instruction::MakeVirtualFunction { .. }
        | Instruction::MakeGenericFunction { .. }
        | Instruction::MakeGenericFunctionFromValue { .. }
        | Instruction::MakeCell => Style::new().cyan(),
        Instruction::LoadDeref(_) | Instruction::LoadCapture(_) | Instruction::CaptureRef(_) => {
            Style::new().blue()
        }
        Instruction::StoreDeref(_) | Instruction::StoreCapture(_) => Style::new().green(),
        Instruction::SendEvent => Style::new().green().bright(),
        Instruction::ContainerLen => Style::new().cyan(),
    }
}

struct Col {
    text: String,
    char_count: usize,
    style: Style,
}

impl From<String> for Col {
    fn from(text: String) -> Self {
        Self {
            char_count: text.chars().count(),
            style: Style::new(),
            text,
        }
    }
}

impl Col {
    fn with_style(mut self, style: Style) -> Self {
        self.style = style;
        self
    }
}

/// Print the bytecode of a function in a readable table format.
///
/// Format is [SOURCE LINE, IP, INSTRUCTION, METADATA]. Something like this:
///
/// ```text
/// 1   0   LOAD_VAR 1        (b)
///     1   JUMP_IF_FALSE 4   (to 5)
///     2   POP
///
/// 2   3   LOAD_CONST 0      (1)
///
/// 4   4   JUMP 3            (to 7)
///     5   POP
///
/// 7   6   LOAD_CONST 1      (2)
///     7   RETURN
/// ```
/// Basically tries to mimic `CPython`'s bytecode disassembly function.
/// Line numbers are shown in the first column, with subsequent instructions
/// from the same line having an empty first column.
///
/// Takes care of calculating how many whitespaces we need to make the table
/// symmetric and returns the entire table.
pub fn display_bytecode(
    function: &Function,
    globals: &[Value],
    objects: Option<&ObjectPool>,
    compile_time_globals: Option<&[bex_vm_types::ConstValue]>,
    use_colors: bool,
) -> String {
    if function.bytecode.instructions.is_empty() {
        return String::new();
    }

    // Row contents.
    let mut rows = Vec::<[Col; 4]>::new();

    // Max width of each column.
    let mut widths = [0; 4];

    // Track the last line number we printed
    let mut last_line: usize = 0;

    // Populate all the rows.
    for instruction_ptr in 0..function.bytecode.instructions.len() {
        let (instruction, metadata) = display_instruction(
            instruction_ptr,
            function,
            globals,
            objects,
            compile_time_globals,
        );

        // decide whether to show the line number
        // since a single line could emit multiple instructions
        let source_line = display_source_line_cell(function, instruction_ptr, &mut last_line);

        let instr_style = instruction_style(&function.bytecode.instructions[instruction_ptr]);

        // Table format is [LINE, IP, INSTR, META].
        let row = [
            Col::from(source_line),
            Col::from(instruction_ptr.to_string()),
            Col::from(instruction).with_style(instr_style),
            Col::from(metadata),
        ];

        // Now calculate the max width of each column.
        for (i, col) in row.iter().enumerate() {
            if col.char_count > widths[i] {
                widths[i] = col.char_count;
            }
        }

        rows.push(row);
    }

    let mut table = String::new();

    // Print the table.
    for (i, row) in rows.iter().enumerate() {
        // Separate bytecode instructions by source line numbers. This checks
        // that the source line has changed compared to the previous
        // instruction.
        if i > 0 && !rows[i][0].text.is_empty() && rows[i - 1][0].text != rows[i][0].text {
            table.push('\n');
        }

        // Build the row.
        for (j, col) in row.iter().enumerate() {
            let mut width = widths[j];

            // First three columns have a margin. Last column doesn't need anything.
            if j < row.len() - 1 {
                width += COLUMN_MARGIN;
            } else {
                width = 0;
            }

            // Apply color based on column, only if output is to a TTY.
            // ANSI codes don't change the visual character count, so we
            // pad based on `col.char_count` after pushing the styled text.
            if use_colors {
                let style = match j {
                    0 => Style::new().black().bright(), // Line numbers in gray
                    1 => Style::new().white(),          // IP in white
                    2 => col.style.clone(),             // Instruction with type-based color
                    3 => Style::new().cyan().bright(),  // Metadata in cyan
                    _ => Style::new(),
                };
                table.push_str(&style.apply_to(&col.text).to_string());
            } else {
                table.push_str(&col.text);
            }
            for _ in col.char_count..width {
                table.push(' ');
            }
        }

        table.push('\n');
    }

    table
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
        Instruction::CallWithRuntimeId { .. } => format!("call_with_runtime_id {}", meta_str(&"")),
        Instruction::CallIndirect => "call_indirect".to_string(),
        Instruction::CallIndirectWithRuntimeId => "call_indirect_with_runtime_id".to_string(),
        Instruction::VirtualCall { nargs, ntypeargs } => {
            format!("virtual_call nargs={nargs} ntypeargs={ntypeargs}")
        }
        Instruction::VirtualCallWithRuntimeId { nargs, ntypeargs } => {
            format!("virtual_call_with_runtime_id nargs={nargs} ntypeargs={ntypeargs}")
        }
        Instruction::SysOp(_) => format!("sys_op {}", meta_str(&"")),
        Instruction::SysOpWithRuntimeId(_) => format!("sys_op_with_runtime_id {}", meta_str(&"")),
        Instruction::Spawn => "spawn".to_string(),
        Instruction::Await => "await".to_string(),
        Instruction::AwaitAny => "await_any".to_string(),

        // --- Control ---
        Instruction::Return => "return".to_string(),
        Instruction::Unreachable => "unreachable".to_string(),

        // --- Type introspection ---
        Instruction::Discriminant => "discriminant".to_string(),
        Instruction::TypeTag => "type_tag".to_string(),
        Instruction::RuntimeIsType => "runtime_is_type".to_string(),
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
                .match_hash_tables
                .get(*table_idx)
                .map(|t| t.key_names.join(", "))
                .unwrap_or_default();
            format!("dense_tag [{names}]")
        }
        Instruction::ThrowIfPanic => "throw_if_panic".to_string(),

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

/// Display a full program in the specified format.
///
/// [`BytecodeFormat::Textual`] produces human-readable assembly with labels
/// (the default for snapshots):
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
/// [`BytecodeFormat::Expanded`] shows raw bytecode addresses, source lines,
/// raw operand indices, and metadata annotations.
pub fn display_program(functions: &[(String, &Function)], format: BytecodeFormat) -> String {
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

        let body = match format {
            BytecodeFormat::Textual => display_bytecode_textual(func),
            BytecodeFormat::Expanded => display_bytecode_expanded(func),
        };
        output.push_str(&body);
        output.push_str("}\n");
    }

    output
}

/// Display bytecode in expanded raw format with source lines, addresses,
/// raw operand indices, and metadata annotations.
///
/// Format is `[SOURCE LINE, ADDRESS, RAW INSTRUCTION, METADATA]`:
///
/// ```text
///   1    0    load_const 0          ("HelloWorld")
///        1    load_var 0            (name)
///        2    load_const 1          ("name")
///        3    alloc_map 1
///        4    call 5                (ai.Agent.run)
///        5    return
/// ```
///
/// Source lines are only shown when they change. A blank line separates
/// groups of instructions from different source lines.
fn display_bytecode_expanded(function: &Function) -> String {
    let instructions = &function.bytecode.instructions;

    if instructions.is_empty() {
        return String::new();
    }

    // Build all rows: [source_line, address, raw_instruction, metadata].
    let mut rows = Vec::<[Col; 4]>::with_capacity(instructions.len());
    let mut widths = [0usize; 4];
    let mut last_line: usize = 0;

    for (ip, instruction) in instructions.iter().enumerate() {
        // Source line column (only when it changes).
        let source_line = display_source_line_cell(function, ip, &mut last_line);

        // Address column.
        let address = ip.to_string();

        // Raw instruction with numeric operands (lowercase).
        let raw = instruction.to_string().to_lowercase();

        // Metadata column (resolved names, jump targets, etc).
        let metadata = display_expanded_metadata(ip, instruction, function);

        let row = [
            Col::from(source_line),
            Col::from(address),
            Col::from(raw),
            Col::from(metadata),
        ];

        for (i, col) in row.iter().enumerate() {
            widths[i] = widths[i].max(col.char_count);
        }

        rows.push(row);
    }

    // Render the table with 2-space indent.
    let mut output = String::new();

    for (i, row) in rows.iter().enumerate() {
        // Blank line between source line groups.
        if i > 0 && !rows[i][0].text.is_empty() && rows[i - 1][0].text != rows[i][0].text {
            output.push('\n');
        }

        output.push_str("  ");
        for (j, col) in row.iter().enumerate() {
            let width = if j < row.len() - 1 {
                widths[j] + COLUMN_MARGIN
            } else {
                0
            };

            output.push_str(&col.text);
            for _ in col.char_count..width {
                output.push(' ');
            }
        }

        output.push('\n');
    }

    output
}

/// Build the metadata annotation column for expanded format.
///
/// Returns resolved operand names in parentheses, or jump target information.
/// Returns an empty string when no metadata is relevant.
fn display_expanded_metadata(ip: usize, instruction: &Instruction, function: &Function) -> String {
    let meta = function
        .bytecode
        .meta
        .get(ip)
        .and_then(|m| m.operand.as_ref());

    match instruction {
        // Instructions with resolved operand names from InstructionMeta.
        Instruction::LoadConst(_)
        | Instruction::LoadVar(_)
        | Instruction::StoreVar(_)
        | Instruction::StoreVarLoadVar(_)
        | Instruction::LoadGlobal(_)
        | Instruction::StoreGlobal(_)
        | Instruction::LoadField(_)
        | Instruction::VirtualLoadField(_)
        | Instruction::VirtualStoreField(_)
        | Instruction::StoreField(_)
        | Instruction::InitField(_)
        | Instruction::InitSpread(_)
        | Instruction::Call { .. }
        | Instruction::CallWithRuntimeId { .. }
        | Instruction::SysOp(_)
        | Instruction::SysOpWithRuntimeId(_)
        | Instruction::AllocInstance { .. }
        | Instruction::InitInstance(_)
        | Instruction::AllocVariant(_) => meta
            .map(|m| format!("({})", sanitize_operand_text(m.as_str())))
            .unwrap_or_default(),

        // Jumps: show absolute target address.
        Instruction::Jump(offset)
        | Instruction::PopJumpIfFalse(offset)
        | Instruction::PopJumpIfTrue(offset)
        | Instruction::JumpIfFalseOrPop(offset)
        | Instruction::JumpIfTrueOrPop(offset)
        | Instruction::JumpIfNotNullOrPop(offset)
        | Instruction::JumpIfFalse(offset) => {
            let target = ip.wrapping_add_signed(*offset);
            format!("(to {target})")
        }

        // Jump tables: show all target addresses.
        Instruction::JumpTable(table_idx) => {
            let default_target =
                ip.wrapping_add_signed(function.bytecode.jump_tables[*table_idx].default);
            let mut entries = Vec::new();
            if let Some(table) = function.bytecode.jump_tables.get(*table_idx) {
                for entry in &table.offsets {
                    match entry {
                        Some(offset) => {
                            let target = ip.wrapping_add_signed(*offset);
                            entries.push(format!("to {target}"));
                        }
                        None => entries.push("_".to_string()),
                    }
                }
            }
            if entries.is_empty() {
                format!("(default to {default_target})")
            } else {
                format!("([{}], default to {default_target})", entries.join(", "))
            }
        }

        // All other instructions: no metadata.
        _ => String::new(),
    }
}

/// Disassemble compact bytecode into human-readable text.
///
/// Produces output in the format:
/// ```text
/// 0000  LOAD_INT_SMALL          42
/// 0002  LOAD_VAR                0
/// 0007  ADD
/// 0008  RETURN
/// ```
///
/// Jump offsets are shown as both the raw relative offset and the resolved
/// absolute byte address target in parentheses.
#[allow(
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::cast_possible_truncation,
    clippy::cast_lossless
)]
pub fn display_compact_bytecode(
    compact: &CompactCode,
    constants: &[ConstValue],
    f: &mut impl std::fmt::Write,
) -> std::fmt::Result {
    let code = &compact.code;
    let mut pc = 0;

    while pc < code.len() {
        let offset = pc;
        let op = OpCode::try_from(code[pc]).map_err(|_| std::fmt::Error)?;
        pc += 1;

        write!(f, "{offset:04}  {op:<24}")?;

        match op {
            // Unit ops: no operands
            OpCode::Return
            | OpCode::Await
            | OpCode::AwaitAny
            | OpCode::Throw
            | OpCode::Rethrow
            | OpCode::LoadArrayElement
            | OpCode::LoadMapElement
            | OpCode::StoreArrayElement
            | OpCode::StoreMapElement
            | OpCode::CallIndirect
            | OpCode::CallIndirectWithRuntimeId
            | OpCode::Discriminant
            | OpCode::TypeTag
            | OpCode::Truthy
            | OpCode::RuntimeIsType
            | OpCode::ThrowIfPanic
            | OpCode::Unreachable
            | OpCode::MakeCell
            | OpCode::SendEvent
            | OpCode::ContainerLen
            | OpCode::Spawn
            | OpCode::Add
            | OpCode::Sub
            | OpCode::Mul
            | OpCode::Div
            | OpCode::Mod
            | OpCode::BitAnd
            | OpCode::BitOr
            | OpCode::BitXor
            | OpCode::Shl
            | OpCode::Shr
            | OpCode::Eq
            | OpCode::NotEq
            | OpCode::Lt
            | OpCode::LtEq
            | OpCode::Gt
            | OpCode::GtEq
            | OpCode::AddInt
            | OpCode::SubInt
            | OpCode::MulInt
            | OpCode::DivInt
            | OpCode::ModInt
            | OpCode::AddFloat
            | OpCode::SubFloat
            | OpCode::MulFloat
            | OpCode::DivFloat
            | OpCode::AddBigint
            | OpCode::SubBigint
            | OpCode::MulBigint
            | OpCode::DivBigint
            | OpCode::ModBigint
            | OpCode::BitAndBigint
            | OpCode::BitOrBigint
            | OpCode::BitXorBigint
            | OpCode::ShlBigint
            | OpCode::ShrBigint
            | OpCode::CmpIntEq
            | OpCode::CmpIntNotEq
            | OpCode::CmpIntLt
            | OpCode::CmpIntLtEq
            | OpCode::CmpIntGt
            | OpCode::CmpIntGtEq
            | OpCode::CmpFloatEq
            | OpCode::CmpFloatNotEq
            | OpCode::CmpFloatLt
            | OpCode::CmpFloatLtEq
            | OpCode::CmpFloatGt
            | OpCode::CmpFloatGtEq
            | OpCode::CmpBigintEq
            | OpCode::CmpBigintNotEq
            | OpCode::CmpBigintLt
            | OpCode::CmpBigintLtEq
            | OpCode::CmpBigintGt
            | OpCode::CmpBigintGtEq
            | OpCode::Not
            | OpCode::Neg
            | OpCode::LoadNull
            | OpCode::LoadTrue
            | OpCode::LoadFalse => {
                writeln!(f)?;
            }

            OpCode::LoadIntSmall => {
                let val = read_i8(code, &mut pc);
                writeln!(f, "{val}")?;
            }

            // Single u32 operand with constant annotation
            OpCode::LoadConst => {
                let idx = read_u32(code, &mut pc);
                if let Some(c) = constants.get(idx as usize) {
                    writeln!(f, "{idx}  ; {c:?}")?;
                } else {
                    writeln!(f, "{idx}")?;
                }
            }

            // Single u32 operand (index/slot)
            OpCode::LoadVar
            | OpCode::StoreVar
            | OpCode::StoreVarLoadVar
            | OpCode::LoadGlobal
            | OpCode::StoreGlobal
            | OpCode::LoadField
            | OpCode::VirtualLoadField
            | OpCode::VirtualStoreField
            | OpCode::StoreField
            | OpCode::InitField
            | OpCode::InitSpread
            | OpCode::Pop
            | OpCode::Copy
            | OpCode::AllocArray
            | OpCode::AllocMap
            | OpCode::InitInstance
            | OpCode::AllocVariant
            | OpCode::SysOp
            | OpCode::SysOpWithRuntimeId
            | OpCode::IsType
            | OpCode::DenseTag
            | OpCode::LoadType
            | OpCode::BindType
            | OpCode::LoadCurrentPackage
            | OpCode::MakeBoundMethod
            | OpCode::LoadDeref
            | OpCode::StoreDeref
            | OpCode::LoadCapture
            | OpCode::StoreCapture
            | OpCode::CaptureRef => {
                let val = read_u32(code, &mut pc);
                writeln!(f, "{val}")?;
            }

            // Jump i32 operand: show relative offset and resolved absolute target
            OpCode::Jump
            | OpCode::PopJumpIfFalse
            | OpCode::JumpIfFalse
            | OpCode::PopJumpIfTrue
            | OpCode::JumpIfFalseOrPop
            | OpCode::JumpIfTrueOrPop
            | OpCode::JumpIfNotNullOrPop => {
                let offset_val = read_i32(code, &mut pc);
                let target = (pc as i64 + offset_val as i64) as usize;
                writeln!(f, "{offset_val:+}  (-> {target:04})")?;
            }

            OpCode::JumpTable => {
                let table_idx = read_u32(code, &mut pc);
                let default_offset = read_i32(code, &mut pc);
                let target = (pc as i64 + default_offset as i64) as usize;
                writeln!(
                    f,
                    "table={table_idx}  default={default_offset:+} (-> {target:04})"
                )?;
            }

            OpCode::Call | OpCode::CallWithRuntimeId => {
                let callee = read_u32(code, &mut pc);
                let ntypeargs = read_u16(code, &mut pc);
                writeln!(f, "callee={callee}  ntypeargs={ntypeargs}")?;
            }

            OpCode::AllocInstance => {
                let class_obj = read_u32(code, &mut pc);
                let ntypeargs = read_u16(code, &mut pc);
                writeln!(f, "class={class_obj}  ntypeargs={ntypeargs}")?;
            }

            OpCode::MakeGenericFunction => {
                let function = read_u32(code, &mut pc);
                let ntypeargs = read_u16(code, &mut pc);
                writeln!(f, "function={function}  ntypeargs={ntypeargs}")?;
            }

            OpCode::MakeGenericFunctionFromValue
            | OpCode::MakeVirtualBoundMethod
            | OpCode::MakeVirtualFunction => {
                let ntypeargs = read_u16(code, &mut pc);
                writeln!(f, "ntypeargs={ntypeargs}")?;
            }

            OpCode::VirtualCall | OpCode::VirtualCallWithRuntimeId => {
                let nargs = read_u16(code, &mut pc);
                let ntypeargs = read_u16(code, &mut pc);
                writeln!(f, "nargs={nargs} ntypeargs={ntypeargs}")?;
            }

            OpCode::MakeClosure => {
                let obj_idx = read_u32(code, &mut pc);
                let capture_count = read_u16(code, &mut pc);
                let ntypeargs = read_u16(code, &mut pc);
                writeln!(
                    f,
                    "obj={obj_idx}  captures={capture_count}  ntypeargs={ntypeargs}"
                )?;
            }

            // Two u32 operands (operand-movement superinstructions)
            OpCode::LoadVar2 | OpCode::StoreVar2 | OpCode::NarrowBind => {
                let a = read_u32(code, &mut pc);
                let b = read_u32(code, &mut pc);
                writeln!(f, "{a} {b}")?;
            }
        }
    }
    Ok(())
}
