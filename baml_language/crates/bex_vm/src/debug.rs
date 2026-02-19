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

use std::io::IsTerminal;

use bex_vm_types::{
    HeapPtr, StackIndex,
    bytecode::Instruction,
    indexable::{GlobalPool, ObjectPool},
    types::{Function, Object, Value},
};
use colored::{Color, Colorize};

use crate::indexable::EvalStack;

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
pub fn display_instruction(
    instruction_ptr: usize,
    function: &Function,
    stack: &EvalStack,
    globals: &GlobalPool,
    objects: Option<&ObjectPool>,
    compile_time_globals: Option<&[bex_vm_types::ConstValue]>,
) -> (String, String) {
    let instruction = &function.bytecode.instructions[instruction_ptr];

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
            // Prefer runtime globals, fall back to compile-time lookup
            if index.raw() < globals.len() {
                format!("({})", display_value(&globals[*index]))
            } else if let (Some(ct_globals), Some(objs)) = (compile_time_globals, objects) {
                // At compile time, look up the global value then resolve to object
                if let Some(const_val) = ct_globals.get(index.raw()) {
                    format!("({})", display_const_value(const_val, Some(objs)))
                } else {
                    format!("(global {})", index.raw())
                }
            } else {
                format!("(global {})", index.raw())
            }
        }
        Instruction::Call(callee) => {
            if callee.raw() < globals.len() {
                format!("({})", display_value(&globals[*callee]))
            } else if let (Some(ct_globals), Some(objs)) = (compile_time_globals, objects) {
                if let Some(const_val) = ct_globals.get(callee.raw()) {
                    format!("({})", display_const_value(const_val, Some(objs)))
                } else {
                    format!("(global {})", callee.raw())
                }
            } else {
                format!("(global {})", callee.raw())
            }
        }
        Instruction::LoadVar(index)
        | Instruction::StoreVar(index)
        | Instruction::Watch(index)
        | Instruction::Unwatch(index)
        | Instruction::Notify(index) => {
            format!(
                "({})",
                function
                    .locals_in_scope
                    .get(function.bytecode.scopes[instruction_ptr])
                    .and_then(|locals| locals.get(*index))
                    .unwrap_or(&"?".to_string())
            )
        }
        Instruction::LoadField(index) | Instruction::StoreField(index) => 'field: {
            // When the compiler calls this, there's no runtime stack so it's
            // not possible to get instruction parameters from the stack.
            // TODO: Figure out a way to get this information without running
            // the VM. When the compiler emits instructions, it could append
            // some metadata to each one of them, simplifying this code a lot
            // since the VM at runtime would only have to print the stack. All
            // instructions with metadata would be provided by the compiler.
            if stack.is_empty() {
                break 'field String::new();
            }

            // TODO: prevent panic here

            let Value::Object(reference) = stack[StackIndex::from_raw(stack.len() - 2)] else {
                break 'field String::from("(ERROR: value not an object)");
            };

            // SAFETY: During debug display, we assume the pointer is valid
            let instance = unsafe { reference.get() };
            let Object::Instance(instance) = instance else {
                break 'field String::from("(ERROR: value not an instance)");
            };

            // SAFETY: During debug display, we assume the pointer is valid
            let class = unsafe { instance.class.get() };
            let Object::Class(class) = class else {
                break 'field String::from("(ERROR: class not found)");
            };

            format!("({})", class.fields[*index].name)
        }
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
        | Instruction::DispatchFuture(_)
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
pub fn display_value(value: &Value) -> String {
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
            Object::String(s) => format!("\"{s}\""),
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
    stack: &EvalStack,
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
            stack,
            globals,
            objects,
            compile_time_globals,
        );

        // decide whether to show the line number
        // since a single line could emit multiple instructions
        let source_line = match function.bytecode.source_lines.get(instruction_ptr) {
            Some(line) if last_line != *line => {
                last_line = *line;
                line.to_string()
            }
            _ => String::new(),
        };

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

/// Prints the dissassembly of a function.
#[allow(clippy::print_stderr)] // intentional debug output for disassembly
pub fn disassemble(function: &Function, stack: &EvalStack, globals: &GlobalPool) {
    let use_colors = std::io::stdout().is_terminal();

    // At runtime, resolved_constants has HeapPtr that can be dereferenced,
    // so we don't need the ObjectPool or compile-time globals
    let disassembly = display_bytecode(function, stack, globals, None, None, use_colors);

    eprintln!("{disassembly}");
}
