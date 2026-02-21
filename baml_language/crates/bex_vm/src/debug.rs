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
    HeapPtr,
    bytecode::Instruction,
    indexable::{GlobalIndex, GlobalPool, ObjectPool},
    types::{Function, Object, Value},
};
use colored::{Color, Colorize};

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
    globals: &GlobalPool,
    objects: Option<&ObjectPool>,
    compile_time_globals: Option<&[bex_vm_types::ConstValue]>,
) -> String {
    // Prefer runtime globals.
    if index.raw() < globals.len() {
        return format!("({})", display_value(&globals[index]));
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
    globals: &GlobalPool,
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
        Instruction::NotifyBlock(block_index) => {
            if let Some(notification) = function.block_notifications.get(*block_index) {
                format!("({})", &notification.block_name)
            } else {
                format!("(invalid block index: {block_index})")
            }
        }
        Instruction::LoadConst(index) => {
            // Prefer resolved_constants (runtime), fall back to constants (compile-time)
            if let Some(value) = function.bytecode.resolved_constants.get(*index) {
                format!("({})", display_value(value))
            } else if let Some(const_value) = function.bytecode.constants.get(*index) {
                format!("({})", display_const_value(const_value, objects))
            } else {
                format!("(const {index})")
            }
        }
        Instruction::LoadGlobal(index) | Instruction::StoreGlobal(index) => {
            display_global_ref(*index, globals, objects, compile_time_globals)
        }
        Instruction::Call(callee) | Instruction::DispatchFuture(callee) => {
            display_global_ref(*callee, globals, objects, compile_time_globals)
        }
        Instruction::LoadVar(index)
        | Instruction::StoreVar(index)
        | Instruction::Watch(index)
        | Instruction::Unwatch(index)
        | Instruction::Notify(index) => match function.local_names.get(*index) {
            Some(name) => format!("({name})"),
            None => "(?)".to_string(),
        },
        Instruction::LoadField(_) | Instruction::StoreField(_) => operand_meta
            .map(|m| format!("({})", m.as_str()))
            .unwrap_or_default(),
        Instruction::Jump(offset) | Instruction::PopJumpIfFalse(offset) => {
            format!("(to {})", instruction_ptr.wrapping_add_signed(*offset))
        }
        Instruction::AllocInstance(index) | Instruction::AllocVariant(index) => {
            // Look up the class/enum from the compile-time ObjectPool if available
            if let Some(objs) = objects {
                format!("({})", display_object_from_pool(index.raw(), objs))
            } else {
                "(object)".to_string()
            }
        }

        Instruction::VizEnter(index) | Instruction::VizExit(index) => {
            if let Some(node) = function.viz_nodes.get(*index) {
                format!("({})", &node.label)
            } else {
                format!("(invalid viz index: {index})")
            }
        }
        Instruction::JumpTable { table_idx, default } => {
            format!("(table {table_idx}, default {default:+})")
        }
        Instruction::Pop(_)
        | Instruction::Copy(_)
        | Instruction::BinOp(_)
        | Instruction::CmpOp(_)
        | Instruction::UnaryOp(_)
        | Instruction::AllocArray(_)
        | Instruction::AllocMap(_)
        | Instruction::LoadArrayElement
        | Instruction::LoadMapElement
        | Instruction::StoreArrayElement
        | Instruction::StoreMapElement
        | Instruction::Await
        | Instruction::CallIndirect
        | Instruction::Assert
        | Instruction::Discriminant
        | Instruction::TypeTag
        | Instruction::Unreachable
        | Instruction::Return => String::new(),
    };

    (instruction.to_string(), metadata)
}

/// Context aware value display.
///
/// The default display for objects is just a reference number. If we want
/// all the information, we have to dereference the object and call it's
/// `to_string` implementation.
pub(crate) fn display_value(value: &Value) -> String {
    match value {
        Value::Object(ptr) => display_object_ptr(*ptr),
        other => other.to_string(),
    }
}

/// Display a compile-time constant value.
///
/// At compile time, we only have `ConstValue` (with `ObjectIndex` for objects).
/// If `ObjectPool` is provided, we can resolve object indices to actual values.
fn display_const_value(value: &bex_vm_types::ConstValue, objects: Option<&ObjectPool>) -> String {
    match value {
        bex_vm_types::ConstValue::Null => "null".to_string(),
        bex_vm_types::ConstValue::Int(i) => i.to_string(),
        bex_vm_types::ConstValue::Float(f) => f.to_string(),
        bex_vm_types::ConstValue::Bool(b) => b.to_string(),
        bex_vm_types::ConstValue::Object(idx) => {
            if let Some(objs) = objects {
                display_object_from_pool(idx.raw(), objs)
            } else {
                format!("<object {}>", idx.raw())
            }
        }
    }
}

/// Display an object from the compile-time `ObjectPool`.
fn display_object_from_pool(index: usize, objects: &ObjectPool) -> String {
    if let Some(obj) = objects.get(index) {
        match obj {
            Object::String(s) => {
                let escaped = s
                    .replace('\\', "\\\\")
                    .replace('\n', "\\n")
                    .replace('\r', "\\r")
                    .replace('\t', "\\t");
                format!("\"{escaped}\"")
            }
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

/// Get color for instruction based on its type
fn instruction_color(instruction: &Instruction) -> Color {
    match instruction {
        Instruction::NotifyBlock(_) => Color::BrightYellow,
        Instruction::LoadConst(_)
        | Instruction::LoadVar(_)
        | Instruction::LoadGlobal(_)
        | Instruction::LoadField(_)
        | Instruction::LoadArrayElement
        | Instruction::LoadMapElement => Color::Blue,
        Instruction::StoreVar(_)
        | Instruction::StoreGlobal(_)
        | Instruction::StoreField(_)
        | Instruction::StoreArrayElement
        | Instruction::StoreMapElement => Color::Green,
        Instruction::BinOp(_) | Instruction::CmpOp(_) | Instruction::UnaryOp(_) => {
            Color::BrightBlue
        }
        Instruction::Jump(_) | Instruction::PopJumpIfFalse(_) | Instruction::JumpTable { .. } => {
            Color::Yellow
        }
        Instruction::Call(_) | Instruction::CallIndirect => Color::Magenta,
        Instruction::Assert | Instruction::Return | Instruction::Pop(_) | Instruction::Copy(_) => {
            Color::Red
        }
        Instruction::AllocMap(_)
        | Instruction::AllocInstance(_)
        | Instruction::AllocVariant(_)
        | Instruction::AllocArray(_) => Color::Cyan,
        Instruction::DispatchFuture(_) | Instruction::Await => Color::BrightGreen,
        Instruction::Watch(_) | Instruction::Unwatch(_) | Instruction::Notify(_) => {
            Color::BrightRed
        }
        Instruction::VizEnter(_) | Instruction::VizExit(_) => Color::BrightYellow,
        Instruction::Discriminant | Instruction::TypeTag => Color::BrightBlue,
        Instruction::Unreachable => Color::BrightRed,
    }
}

struct Col {
    text: String,
    char_count: usize,
    color: Color,
}

impl From<String> for Col {
    fn from(text: String) -> Self {
        Self {
            char_count: text.chars().count(),
            color: Color::White,
            text,
        }
    }
}

impl Col {
    fn with_color(mut self, color: Color) -> Self {
        self.color = color;
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
    globals: &GlobalPool,
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

        let instruction_color = instruction_color(&function.bytecode.instructions[instruction_ptr]);

        // Table format is [LINE, IP, INSTR, META].
        let row = [
            Col::from(source_line),
            Col::from(instruction_ptr.to_string()),
            Col::from(instruction).with_color(instruction_color),
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

            let mut colored_text = col.text.normal();

            // Apply color based on column, only if output is to a TTY
            if use_colors {
                colored_text = match j {
                    0 => col.text.bright_black(),   // Line numbers in gray
                    1 => col.text.white(),          // IP in white
                    2 => col.text.color(col.color), // Instruction with type-based color
                    3 => col.text.bright_cyan(),    // Metadata in cyan
                    _ => col.text.normal(),
                }
            }

            // For colored strings, we need to use the actual character count
            // not the length with ANSI codes. Also, `to_string` has to be
            // called here so that ANSI codes are inserted.
            table.push_str(&colored_text.to_string());
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
            Instruction::Jump(offset) | Instruction::PopJumpIfFalse(offset) => {
                let target = ip.wrapping_add_signed(*offset);
                jump_targets.insert(target);
            }
            Instruction::JumpTable { table_idx, default } => {
                // Default target.
                let default_target = ip.wrapping_add_signed(*default);
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
        function
            .bytecode
            .meta
            .get(ip)
            .and_then(|m| m.operand.as_ref())
            .map(|o| o.as_str().to_string())
            .unwrap_or_else(|| format!("?{fallback}"))
    };

    match instruction {
        // --- Constants ---
        Instruction::LoadConst(_) => format!("load_const {}", meta_str(&"")),

        // --- Variables ---
        Instruction::LoadVar(idx) => format!("load_var {}", meta_str(idx)),
        Instruction::StoreVar(idx) => format!("store_var {}", meta_str(idx)),

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
        Instruction::JumpTable { table_idx, default } => {
            let default_target = ip.wrapping_add_signed(*default);
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

        // --- Operators ---
        Instruction::BinOp(op) => format!("bin_op {op}"),
        Instruction::CmpOp(op) => format!("cmp_op {op}"),
        Instruction::UnaryOp(op) => format!("unary_op {op}"),

        // --- Allocation ---
        Instruction::AllocArray(n) => format!("alloc_array {n}"),
        Instruction::AllocMap(n) => format!("alloc_map {n}"),
        Instruction::AllocInstance(_) => format!("alloc_instance {}", meta_str(&"")),
        Instruction::AllocVariant(_) => format!("alloc_variant {}", meta_str(&"")),

        // --- Array/Map element access ---
        Instruction::LoadArrayElement => "load_array_element".to_string(),
        Instruction::LoadMapElement => "load_map_element".to_string(),
        Instruction::StoreArrayElement => "store_array_element".to_string(),
        Instruction::StoreMapElement => "store_map_element".to_string(),

        // --- Calls ---
        Instruction::Call(_) => format!("call {}", meta_str(&"")),
        Instruction::CallIndirect => "call_indirect".to_string(),
        Instruction::DispatchFuture(_) => format!("dispatch_future {}", meta_str(&"")),
        Instruction::Await => "await".to_string(),

        // --- Control ---
        Instruction::Return => "return".to_string(),
        Instruction::Assert => "assert".to_string(),
        Instruction::Unreachable => "unreachable".to_string(),

        // --- Watch/Notify ---
        Instruction::Watch(idx) => format!("watch {}", meta_str(idx)),
        Instruction::Unwatch(idx) => format!("unwatch {}", meta_str(idx)),
        Instruction::Notify(idx) => format!("notify {}", meta_str(idx)),
        Instruction::NotifyBlock(block_index) => {
            if let Some(notification) = function.block_notifications.get(*block_index) {
                format!("notify_block {}", notification.block_name)
            } else {
                format!("notify_block {block_index}")
            }
        }

        // --- Visualization ---
        Instruction::VizEnter(index) => {
            if let Some(node) = function.viz_nodes.get(*index) {
                format!("viz_enter {}", node.label)
            } else {
                format!("viz_enter {index}")
            }
        }
        Instruction::VizExit(index) => {
            if let Some(node) = function.viz_nodes.get(*index) {
                format!("viz_exit {}", node.label)
            } else {
                format!("viz_exit {index}")
            }
        }

        // --- Type introspection ---
        Instruction::Discriminant => "discriminant".to_string(),
        Instruction::TypeTag => "type_tag".to_string(),
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
///        4    call 5                (baml.llm.call_llm_function)
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
        | Instruction::LoadGlobal(_)
        | Instruction::StoreGlobal(_)
        | Instruction::LoadField(_)
        | Instruction::StoreField(_)
        | Instruction::Call(_)
        | Instruction::DispatchFuture(_)
        | Instruction::AllocInstance(_)
        | Instruction::AllocVariant(_)
        | Instruction::Watch(_)
        | Instruction::Unwatch(_)
        | Instruction::Notify(_) => meta
            .map(|m| format!("({})", m.as_str()))
            .unwrap_or_default(),

        // Jumps: show absolute target address.
        Instruction::Jump(offset) | Instruction::PopJumpIfFalse(offset) => {
            let target = ip.wrapping_add_signed(*offset);
            format!("(to {target})")
        }

        // Jump tables: show all target addresses.
        Instruction::JumpTable { table_idx, default } => {
            let default_target = ip.wrapping_add_signed(*default);
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

        // Block notifications: show block name.
        Instruction::NotifyBlock(block_index) => function
            .block_notifications
            .get(*block_index)
            .map(|n| format!("({})", n.block_name))
            .unwrap_or_default(),

        // Visualization: show node label.
        Instruction::VizEnter(index) | Instruction::VizExit(index) => function
            .viz_nodes
            .get(*index)
            .map(|n| format!("({})", n.label))
            .unwrap_or_default(),

        // All other instructions: no metadata.
        _ => String::new(),
    }
}
