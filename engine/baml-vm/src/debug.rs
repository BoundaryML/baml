//! VM debugging utilities & helpers.
//!
//! NOTE: Functions here should not take an entire reference to the
//! [`crate::Vm`] because then it will be hard to circumvent the borrow checker
//! in the [`crate::Vm::exec`] loop (which is where we want to use this).
//!
//! Instead, they take read only references to the parts of the [`crate::Vm`]
//! that they need, that way inside the loop we can "destructure" the
//! [`crate::Vm`] and let the compiler know exactly which properties we're
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

use colored::{Color, Colorize};

use crate::{
    bytecode::Instruction,
    indexable::{EvalStack, GlobalPool},
    types::{Function, Object, Value, VizNodeMeta},
    ObjectIndex, ObjectPool, StackIndex,
};

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
    instruction_ptr: isize,
    function: &Function,
    stack: &EvalStack,
    objects: &ObjectPool,
    globals: &GlobalPool,
) -> (String, String) {
    let instruction = &function.bytecode.instructions[instruction_ptr as usize];

    let metadata = match instruction {
        Instruction::LoadConst(index) => format!(
            "({})",
            display_value(&function.bytecode.constants[*index], objects)
        ),
        Instruction::LoadGlobal(index) | Instruction::StoreGlobal(index) => {
            format!("({})", display_value(&globals[*index], objects))
        }
        Instruction::LoadVar(index)
        | Instruction::StoreVar(index)
        | Instruction::Watch(index)
        | Instruction::Notify(index) => {
            format!(
                "({})",
                function
                    .locals_in_scope
                    .get(function.bytecode.scopes[instruction_ptr as usize])
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

            let Object::Instance(instance) = &objects[reference] else {
                break 'field String::from("(ERROR: value not an instance)");
            };

            let Object::Class(class) = &objects[instance.class] else {
                break 'field String::from("(ERROR: class not found)");
            };

            format!("({})", class.field_names[*index])
        }
        Instruction::Jump(offset) | Instruction::JumpIfFalse(offset) => {
            format!("(to {})", instruction_ptr + offset)
        }
        Instruction::VizEnter(index) | Instruction::VizExit(index) => {
            viz_metadata(*index, &function.viz_nodes)
        }
        Instruction::AllocInstance(index) | Instruction::AllocVariant(index) => {
            format!("({})", display_object(objects, *index))
        }

        Instruction::Pop(_)
        | Instruction::Copy(_)
        | Instruction::PopReplace(_)
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
        | Instruction::Call(_)
        | Instruction::Assert
        | Instruction::Return => String::new(),
    };

    (instruction.to_string(), metadata)
}

/// Context aware value display.
///
/// The default display for objects is just a reference number. If we want
/// all the information, we have to dereference the object and call it's
/// `to_string` implementation.
pub fn display_value(value: &Value, objects: &ObjectPool) -> String {
    match value {
        Value::Object(index) => display_object(objects, *index),

        other => other.to_string(),
    }
}

fn display_object(objects: &ObjectPool, index: ObjectIndex) -> String {
    match &objects[index] {
        // This one's a bit tricky to print.
        Object::Instance(instance) => match &objects[instance.class] {
            Object::Class(class) => format!("<{} instance>", class.name),
            // This will most likely never happen, but we're trying not
            // to panic.
            other => format!("<{other} instance>"),
        },

        Object::Variant(variant) => match &objects[variant.enm] {
            Object::Enum(enm) => format!("<{} variant>", enm.name),
            other => format!("<{other} variant>"),
        },

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
        Instruction::Jump(_) | Instruction::JumpIfFalse(_) => Color::Yellow,
        Instruction::Call(_) => Color::Magenta,
        Instruction::Assert
        | Instruction::Return
        | Instruction::Pop(_)
        | Instruction::Copy(_)
        | Instruction::PopReplace(_) => Color::Red,
        Instruction::AllocMap(_)
        | Instruction::AllocInstance(_)
        | Instruction::AllocVariant(_)
        | Instruction::AllocArray(_) => Color::Cyan,
        Instruction::DispatchFuture(_) | Instruction::Await => Color::BrightGreen,
        Instruction::VizEnter(_) | Instruction::VizExit(_) => Color::White,
        Instruction::Watch(_) | Instruction::Notify(_) => Color::BrightRed,
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
/// Basically tries to mimic CPython's bytecode disassembly function.
/// Line numbers are shown in the first column, with subsequent instructions
/// from the same line having an empty first column.
///
/// Takes care of calculating how many whitespaces we need to make the table
/// symmetric and returns the entire table.
pub fn display_bytecode(
    function: &Function,
    stack: &EvalStack,
    objects: &ObjectPool,
    globals: &GlobalPool,
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
        let (instruction, metadata) =
            display_instruction(instruction_ptr as isize, function, stack, objects, globals);

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

fn viz_metadata(index: usize, nodes: &[VizNodeMeta]) -> String {
    match nodes.get(index) {
        Some(node) => {
            let mut metadata = vec![
                format!("node_id={}", node.node_id),
                format!("log_filter_key={}", node.log_filter_key),
                format!("type={:?}", node.node_type),
            ];
            if let Some(parent) = &node.parent_log_filter_key {
                metadata.push(format!("parent_log_filter_key={parent}"));
            }
            if !node.label.is_empty() {
                metadata.push(format!("label=\"{}\"", node.label));
            }
            if let Some(level) = node.header_level {
                metadata.push(format!("level={level}"));
            }
            format!("({})", metadata.join(", "))
        }
        None => format!("(invalid viz index: {index})"),
    }
}

/// Prints the dissassembly of a function.
pub fn disassemble(
    function: &Function,
    stack: &EvalStack,
    objects: &ObjectPool,
    globals: &GlobalPool,
) {
    let use_colors = std::io::stdout().is_terminal();

    let disassembly = display_bytecode(function, stack, objects, globals, use_colors);

    eprintln!("{disassembly}");
}
