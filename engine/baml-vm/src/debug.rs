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

use colored::*;

use crate::{Function, Instruction, Object, Value};

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
    stack: &[Value],
    objects: &[Object],
    globals: &[Value],
) -> (String, String) {
    let instruction = &function.bytecode.instructions[instruction_ptr as usize];

    let metadata = match instruction {
        Instruction::LoadConst(index) => format!(
            "({})",
            display_value(&function.bytecode.constants[*index], objects)
        ),

        // TODO: For this one we need to add some logic to check if it's
        // a function or a global variable. In the case of variables, we
        // have to store the names (potentially in the [`Vm`] struct) and
        // print it.
        Instruction::LoadGlobal(index) | Instruction::StoreGlobal(index) => {
            format!("({})", display_value(&globals[*index], objects))
        }

        Instruction::LoadVar(index) | Instruction::StoreVar(index) => {
            format!(
                "({})",
                function
                    .local_var_names
                    .get(*index)
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

            let Value::Object(reference) = stack[stack.len() - 2] else {
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

        // Classes are also globals, we can get the name from there.
        Instruction::AllocInstance(index) => {
            format!("({})", display_value(&globals[*index], objects))
        }

        Instruction::Pop
        | Instruction::AllocArray(_)
        | Instruction::Call(_)
        | Instruction::EndBlock(_)
        | Instruction::Return => String::new(),
    };

    (instruction.to_string(), metadata)
}

/// Context aware value display.
///
/// The default display for objects is just a reference number. If we want
/// all the information, we have to dereference the object and call it's
/// `to_string` implementation.
pub fn display_value(value: &Value, objects: &[Object]) -> String {
    match value {
        Value::Object(index) => match &objects[*index] {
            // This one's a bit tricky to print.
            Object::Instance(instance) => match &objects[instance.class] {
                Object::Class(class) => format!("<{} instance>", class.name),
                // This will most likely never happen, but we're trying not
                // to panic.
                other => format!("<{other} instance>"),
            },

            other => other.to_string(),
        },

        other => other.to_string(),
    }
}

/// The number of whitespaces that separate each column.
///
/// See [`display_bytecode`] for more information.
const COLUMN_MARGIN: usize = 3;

/// Get color for instruction based on its type
fn get_instruction_color(instruction: &Instruction) -> Color {
    match instruction {
        Instruction::LoadConst(_)
        | Instruction::LoadVar(_)
        | Instruction::LoadGlobal(_)
        | Instruction::LoadField(_) => Color::Blue,
        Instruction::StoreVar(_) | Instruction::StoreGlobal(_) | Instruction::StoreField(_) => {
            Color::Green
        }
        Instruction::Jump(_) | Instruction::JumpIfFalse(_) => Color::Yellow,
        Instruction::Call(_) => Color::Magenta,
        Instruction::Return => Color::Red,
        Instruction::AllocInstance(_) | Instruction::AllocArray(_) => Color::Cyan,
        Instruction::Pop | Instruction::EndBlock(_) => Color::BrightBlack,
    }
}

/// Print the bytecode of a function in a readable table format.
///
/// Format is [LINE, IP, INSTRUCTION, METADATA]. Something like this:
///
/// ```text
/// 1   0   LOAD_VAR 1        (b)
///     1   JUMP_IF_FALSE 4   (to 5)
///     2   POP
/// 2   3   LOAD_CONST 0      (1)
/// 4   4   JUMP 3            (to 7)
///     5   POP
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
    stack: &[Value],
    objects: &[Object],
    globals: &[Value],
) -> String {
    if function.bytecode.instructions.is_empty() {
        return String::new();
    }

    // Row contents. [String, String, String, String]
    let mut rows = Vec::new();
    // Char count of the strings above. [usize, usize, usize, usize]
    let mut chars_count = Vec::new();
    // Max width of each column. [usize, usize, usize, usize]
    let mut widths = [0; 4];
    // Store instruction colors for each row
    let mut row_colors = Vec::new();

    // Track the last line number we printed
    let mut last_line: usize = 0;

    // Populate all the rows.
    for instruction_ptr in 0..function.bytecode.instructions.len() {
        let (instruction, metadata) =
            display_instruction(instruction_ptr as isize, function, stack, objects, globals);

        // Get the source line for this instruction
        let source_line = function
            .bytecode
            .source_lines
            .get(instruction_ptr)
            .map(|&line| line);

        // decide whether to show the line number
        // since a single line could emit multiple instructions
        let line_str = match source_line {
            Some(line) if last_line != line => {
                last_line = line;
                line.to_string()
            }
            _ => String::new(),
        };

        // Table format is [LINE, IP, INSTR, META].
        let row = [line_str, instruction_ptr.to_string(), instruction, metadata];
        let mut char_count = [0, 0, 0, 0];

        // Now calculate the max width of each column.
        for (i, col) in row.iter().enumerate() {
            let width = col.chars().count();

            char_count[i] = width;

            if width > widths[i] {
                widths[i] = width;
            }
        }

        rows.push(row);
        chars_count.push(char_count);
        row_colors.push(get_instruction_color(
            &function.bytecode.instructions[instruction_ptr],
        ));
    }

    let mut table = String::new();

    // Check if stdout is a TTY to determine whether to use colors
    let use_colors = std::io::stdout().is_terminal();

    // Print the table.
    for ((row, char_count), color) in rows.iter().zip(chars_count).zip(row_colors) {
        for (i, col) in row.iter().enumerate() {
            let mut width = widths[i];

            // First three columns have a margin. Last column doesn't need anything.
            if i < row.len() - 1 {
                width += COLUMN_MARGIN;
            } else {
                width = 0;
            }

            // Apply color based on column, only if output is to a TTY
            let colored_text = if use_colors {
                match i {
                    0 => col.bright_black().to_string(),      // Line numbers in gray
                    1 => col.white().to_string(),             // IP in white
                    2 => col.color(color).bold().to_string(), // Instruction with type-based color
                    3 => col.bright_cyan().to_string(),       // Metadata in cyan
                    _ => col.to_string(),
                }
            } else {
                col.to_string() // No colors when not writing to TTY
            };

            // For colored strings, we need to use the actual character count
            // not the length with ANSI codes
            table.push_str(&colored_text);
            for _ in char_count[i]..width {
                table.push(' ');
            }
        }

        table.push('\n');
    }

    table
}
