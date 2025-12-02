//! VM debugging utilities & helpers.
//!
//! This module provides functions to display bytecode in a human-readable format,
//! similar to `CPython`'s bytecode disassembly. It can be used both at runtime
//! for debugging and at compile time for snapshot testing.

use crate::{
    bytecode::Instruction,
    types::{Function, Object, Value},
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
    instruction_ptr: usize,
    function: &Function,
    stack: &[Value],
    objects: &[Object],
    globals: &[Value],
) -> (String, String) {
    let instruction = &function.bytecode.instructions[instruction_ptr];

    let metadata = match instruction {
        Instruction::NotifyBlock(block_index) => {
            // Block notifications aren't stored in the simplified Function struct.
            format!("(block {block_index})")
        }
        Instruction::LoadConst(index) => format!(
            "({})",
            display_value(&function.bytecode.constants[*index], objects)
        ),
        Instruction::LoadGlobal(index) | Instruction::StoreGlobal(index) => {
            if let Some(value) = globals.get(*index) {
                format!("({})", display_value(value, objects))
            } else {
                format!("(global {index})")
            }
        }
        Instruction::LoadVar(index)
        | Instruction::StoreVar(index)
        | Instruction::Watch(index)
        | Instruction::Notify(index) => {
            let scope_idx = function.bytecode.scopes.get(instruction_ptr).copied();
            let name = scope_idx
                .and_then(|s| function.locals_in_scope.get(s))
                .and_then(|locals| locals.get(*index))
                .map(std::string::String::as_str)
                .unwrap_or("?");
            format!("({name})")
        }
        Instruction::LoadField(index) | Instruction::StoreField(index) => 'field: {
            // When the compiler calls this, there's no runtime stack so it's
            // not possible to get instruction parameters from the stack.
            if stack.is_empty() {
                break 'field String::new();
            }

            let Some(Value::Object(obj_idx)) = stack.get(stack.len().saturating_sub(2)) else {
                break 'field String::from("(ERROR: value not an object)");
            };

            let Some(Object::Instance { class_index, .. }) = objects.get(*obj_idx) else {
                break 'field String::from("(ERROR: value not an instance)");
            };

            let Some(Object::Class(class)) = objects.get(*class_index) else {
                break 'field String::from("(ERROR: class not found)");
            };

            format!(
                "({})",
                class
                    .field_names
                    .get(*index)
                    .map(std::string::String::as_str)
                    .unwrap_or("?")
            )
        }
        Instruction::Jump(offset) | Instruction::JumpIfFalse(offset) => {
            let target = instruction_ptr.wrapping_add_signed(*offset);
            format!("(to {target})")
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
pub fn display_value(value: &Value, objects: &[Object]) -> String {
    match value {
        Value::Object(index) => display_object(objects, *index),
        other => other.to_string(),
    }
}

fn display_object(objects: &[Object], index: usize) -> String {
    match objects.get(index) {
        Some(Object::Instance { class_index, .. }) => match objects.get(*class_index) {
            Some(Object::Class(class)) => format!("<{} instance>", class.name),
            Some(other) => format!("<{other} instance>"),
            None => format!("<instance of class {class_index}>"),
        },

        Some(Object::Variant { enum_index, .. }) => match objects.get(*enum_index) {
            Some(Object::Enum(enm)) => format!("<{} variant>", enm.name),
            Some(other) => format!("<{other} variant>"),
            None => format!("<variant of enum {enum_index}>"),
        },

        Some(other) => other.to_string(),
        None => format!("<invalid object {index}>"),
    }
}

/// The number of whitespaces that separate each column.
///
/// See [`display_bytecode`] for more information.
const COLUMN_MARGIN: usize = 3;

struct Col {
    text: String,
    char_count: usize,
}

impl From<String> for Col {
    fn from(text: String) -> Self {
        Self {
            char_count: text.chars().count(),
            text,
        }
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
    stack: &[Value],
    objects: &[Object],
    globals: &[Value],
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
            display_instruction(instruction_ptr, function, stack, objects, globals);

        // decide whether to show the line number
        // since a single line could emit multiple instructions
        let source_line = match function.bytecode.source_lines.get(instruction_ptr) {
            Some(line) if last_line != *line => {
                last_line = *line;
                line.to_string()
            }
            _ => String::new(),
        };

        // Table format is [LINE, IP, INSTR, META].
        let row = [
            Col::from(source_line),
            Col::from(instruction_ptr.to_string()),
            Col::from(instruction),
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
        // Build the row.
        for (j, col) in row.iter().enumerate() {
            let mut width = widths[j];

            // First three columns have a margin. Last column doesn't need anything.
            if j < row.len() - 1 {
                width += COLUMN_MARGIN;
            } else {
                width = 0;
            }

            table.push_str(&col.text);
            for _ in col.char_count..width {
                table.push(' ');
            }
        }

        table.push('\n');

        // Add blank line after each source line group (when source lines are tracked).
        // Check if the next row starts a new source line (has a non-empty line number).
        // When source lines are all 0 (not tracked), no blank lines are added.
        if i + 1 < rows.len() && !rows[i + 1][0].text.is_empty() {
            table.push('\n');
        }
    }

    table
}
