use lsp_types::{Position, Range, TextDocumentItem};
use regex::Regex;
use std::cmp;

const MAX_SAFE_CHAR: u32 = std::u32::MAX; // You can adjust this to mimic your TS MAX_SAFE_VALUE_i32

/// Returns the full range of the document, from the beginning (line 0, character 0)
/// to the “end” (last line with a very large character position).
pub fn full_document_range(contents: &str) -> Range {
    // Compute the number of lines. If the text is empty, assume one line.
    let line_count = if contents.is_empty() {
        1
    } else {
        contents.lines().count()
    };

    let last_line = if line_count > 0 {
        (line_count - 1) as u32
    } else {
        0
    };

    Range {
        start: Position {
            line: 0,
            character: 0,
        },
        end: Position {
            line: last_line,
            character: MAX_SAFE_CHAR,
        },
    }
}

/// Returns the text of the current line (by number) from the document.
/// If the line does not exist, an empty string is returned.
pub fn get_current_line<'a>(contents: &'a str, line: u32) -> &'a str {
    contents.lines().nth(line as usize).unwrap_or("")
}

/// Determines if a given position is the first inside a block.
/// The logic is as follows:
/// - If the trimmed current line is empty, return true.
/// - Otherwise, take the substring _up to_ the given position and check if the last
///   “word” (using a `\w+` search) ends exactly at the position.
pub fn is_first_inside_block(position: &Position, current_line: &str) -> bool {
    if current_line.trim().is_empty() {
        return true;
    }

    // Ensure we don’t slice past the length of the current line.
    let pos = cmp::min(position.character as usize, current_line.len());
    let string_til_position = &current_line[..pos];

    // Find the first occurrence of a word.
    let re = Regex::new(r"\w+").unwrap();
    if let Some(mat) = re.find(string_til_position) {
        // Return true if there is only one word segment and it ends exactly at position.character.
        mat.start() + mat.as_str().len() == string_til_position.len()
    } else {
        true
    }
}

/// Returns the word at the given position in the document. This function:
/// 1. Gets the current line image.
/// 2. Searches backward (up to position.character+1) for a non-space substring (`\S+$`) to
///    locate the beginning of the word.
/// 3. Searches forward (from position.character) for a non-word character (`\W`).
///
/// If no non-word boundary is found after the position, an empty string is returned.
pub fn get_word_at_position(contents: &str, position: &Position) -> String {
    let current_line = get_current_line(contents, position.line);
    let line_len = current_line.len();

    // Clamp position.character to the current line length.
    let pos = cmp::min(position.character as usize, line_len);

    // Search backward from position.character + 1 using a regex for the last non-whitespace sequence.
    let re_begin = Regex::new(r"\S+$").unwrap();
    let slice_end = cmp::min(pos + 1, line_len);
    let substring_before = &current_line[..slice_end];
    let beginning = if let Some(mat) = re_begin.find(substring_before) {
        mat.start()
    } else {
        return "".to_string();
    };

    // Search forward from position.character for the first non-word character.
    let re_end = Regex::new(r"\W").unwrap();
    let substring_after = &current_line[pos..];
    let end = if let Some(mat) = re_end.find(substring_after) {
        mat.start()
    } else {
        substring_after.len()
    };

    let word_end = pos + end;

    if beginning <= word_end && word_end <= current_line.len() {
        current_line[beginning..word_end].to_string()
    } else {
        "".to_string()
    }
}

/// Returns the symbol (a single character) immediately preceding the given position.
/// If the position is at the start of the line, an empty string is returned.
pub fn get_symbol_before_position(contents: &str, position: &Position) -> String {
    if position.character == 0 {
        return "".to_string();
    }
    let current_line = get_current_line(contents, position.line);
    let pos = cmp::min(position.character as usize, current_line.len());
    if pos == 0 {
        return "".to_string();
    }
    // This simple slicing works correctly if the text is ASCII.
    current_line[pos - 1..pos].to_string()
}

/// Computes the Position (line and character) corresponding to a given index in the document’s text.
/// This mimics the TS implementation by iterating over each character up to the given index.
pub fn get_position_from_index(document: &TextDocumentItem, index: usize) -> Position {
    let mut line: u32 = 0;
    let mut character: u32 = 0;
    let mut count = 0;

    for c in document.text.chars() {
        if count >= index {
            break;
        }
        if c == '\n' {
            line += 1;
            character = 0;
        } else {
            character += 1;
        }
        count += 1;
    }

    Position { line, character }
}
