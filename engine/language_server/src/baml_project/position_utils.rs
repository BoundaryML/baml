use std::cmp;

use lsp_types::{Position, Range, TextDocumentItem};
use regex::Regex;

const MAX_SAFE_CHAR: u32 = u32::MAX; // You can adjust this to mimic your TS MAX_SAFE_VALUE_i32

/// Returns the full range of the document, from the beginning (line 0, character 0)
/// to the "end" (last line with a very large character position).
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
pub fn get_current_line(contents: &str, line: u32) -> &str {
    contents.lines().nth(line as usize).unwrap_or("")
}

/// Determines if a given position is the first inside a block.
/// The logic is as follows:
/// - If the trimmed current line is empty, return true.
/// - Otherwise, take the substring _up to_ the given position and check if the last
///   "word" (using a `\w+` search) ends exactly at the position.
pub fn is_first_inside_block(position: &Position, current_line: &str) -> bool {
    if current_line.trim().is_empty() {
        return true;
    }

    // Ensure we don't slice past the length of the current line, using character counts.
    let char_count_in_line = current_line.chars().count();
    let clamped_char_pos = cmp::min(position.character as usize, char_count_in_line);

    // Convert character position to byte offset for slicing.
    let byte_offset_at_clamped_char_pos = current_line
        .char_indices()
        .nth(clamped_char_pos)
        .map_or(current_line.len(), |(idx, _)| idx);

    let string_til_position = &current_line[..byte_offset_at_clamped_char_pos];

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
    if current_line.is_empty() {
        return "".to_string();
    }

    let char_count_in_line = current_line.chars().count();
    // `position.character` is a 0-indexed character offset. Clamp it.
    let clamped_char_idx = cmp::min(position.character as usize, char_count_in_line);

    // Part 1: Search backward to find the start of the word.
    // Slice for backward search extends up to character `clamped_char_idx + 1` (exclusive).
    let char_len_for_backward_search = cmp::min(clamped_char_idx + 1, char_count_in_line);
    let byte_len_for_backward_search_slice = current_line
        .char_indices()
        .nth(char_len_for_backward_search)
        .map_or(current_line.len(), |(idx, _)| idx);
    let text_for_backward_search = &current_line[..byte_len_for_backward_search_slice];

    // Search backward from position.character + 1 using a regex for the last non-whitespace sequence.
    let re_begin = Regex::new(r"\S+$").unwrap();
    let word_start_byte_idx = match re_begin.find(text_for_backward_search) {
        Some(mat) => mat.start(),
        None => return "".to_string(),
    };

    // Convert `clamped_char_idx` (character index) to a byte index for the LSP `position.character`.
    // This `pos` will be used as the starting point for the forward search.
    let pos = current_line
        .char_indices()
        .nth(clamped_char_idx)
        .map_or(current_line.len(), |(idx, _)| idx);

    // Search forward from position.character for the first non-word character.
    let re_end = Regex::new(r"\W").unwrap();
    let substring_after = &current_line[pos..];
    let end = if let Some(mat) = re_end.find(substring_after) {
        mat.start()
    } else {
        substring_after.len()
    };

    let word_end = pos + end;

    if word_start_byte_idx <= word_end && word_end <= current_line.len() {
        current_line[word_start_byte_idx..word_end].to_string()
    } else {
        "".to_string()
    }
}

/// Returns the symbol (a single character) immediately preceding the given position.
/// If the position is at the start of the line, an empty string is returned.
pub fn get_symbol_before_position(contents: &str, position: &Position) -> String {
    let current_line = get_current_line(contents, position.line);
    // position.character is a 0-indexed character offset.
    let char_cursor_pos = position.character as usize;

    if char_cursor_pos == 0 {
        return "".to_string();
    }

    // Clamp the character cursor position against the actual number of characters in the line.
    // This ensures that if position.character is, for example, 5, but the line only has 3 chars,
    // we don't panic. We want the character at index (clamped_char_cursor_pos - 1).
    let num_chars_in_line = current_line.chars().count();

    // If the effective cursor position is beyond the line's character length,
    // or if it's at the very beginning (char_cursor_pos == 0, handled above),
    // there's no valid preceding character to get by simple indexing from char_cursor_pos.
    // We are interested in the character at `char_cursor_pos - 1`.
    if char_cursor_pos > num_chars_in_line {
        // If cursor is effectively beyond the line, the "preceding" character would be the last one.
        // So we try to get char at num_chars_in_line - 1.
        if num_chars_in_line == 0 {
            return "".to_string(); // Empty line
        }
        return current_line
            .chars()
            .nth(num_chars_in_line - 1)
            .map_or("".to_string(), |ch| ch.to_string());
    }

    // At this point, 0 < char_cursor_pos <= num_chars_in_line.
    // We want the character at index (char_cursor_pos - 1).
    current_line
        .chars()
        .nth(char_cursor_pos - 1)
        .map_or("".to_string(), |ch| ch.to_string())
}

/// Computes the Position (line and character) corresponding to a given index in the document's text.
/// This mimics the TS implementation by iterating over each character up to the given index.
pub fn get_position_from_index(document: &TextDocumentItem, index: usize) -> Position {
    let mut line: u32 = 0;
    let mut character: u32 = 0;

    for (count, c) in document.text.chars().enumerate() {
        if count >= index {
            break;
        }
        if c == '\n' {
            line += 1;
            character = 0;
        } else {
            character += 1;
        }
    }

    Position { line, character }
}
