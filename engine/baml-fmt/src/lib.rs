#![allow(dead_code)]

mod lint;
mod validate;

use baml_lib::{internal_baml_parser_database::ast, SourceFile};

use lsp_types::{Position, Range};

pub fn call_llm(schema: String) -> String {
    schema
}

pub fn lint(schema: String) -> String {
    lint::run(&schema)
}

pub fn validate(validate_params: String) -> Result<(), String> {
    validate::validate(&validate_params)
}

/// The LSP position is expressed as a (line, col) tuple, but our pest-based parser works with byte
/// offsets. This function converts from an LSP position to a pest byte offset. Returns `None` if
/// the position has a line past the end of the document, or a character position past the end of
/// the line.
pub(crate) fn position_to_offset(position: &Position, document: &str) -> Option<usize> {
    let mut offset = 0;
    let mut line_offset = position.line;
    let mut character_offset = position.character;
    let mut chars = document.chars();

    while line_offset > 0 {
        loop {
            match chars.next() {
                Some('\n') => {
                    offset += 1;
                    break;
                }
                Some(_) => {
                    offset += 1;
                }
                None => return Some(offset),
            }
        }

        line_offset -= 1;
    }

    while character_offset > 0 {
        match chars.next() {
            Some('\n') | None => return Some(offset),
            Some(_) => {
                offset += 1;
                character_offset -= 1;
            }
        }
    }

    Some(offset)
}

#[track_caller]
/// Converts an LSP range to a span.
pub(crate) fn range_to_span(range: Range, document: &str) -> ast::Span {
    let start = position_to_offset(&range.start, document).unwrap();
    let end = position_to_offset(&range.end, document).unwrap();

    ast::Span::new(
        SourceFile::from(("<unknown>".into(), "contents".to_string())),
        start,
        end,
    )
}

/// Gives the LSP position right after the given span.
pub(crate) fn position_after_span(span: ast::Span, document: &str) -> Position {
    offset_to_position(span.end - 1, document)
}

/// Converts a byte offset to an LSP position, if the given offset
/// does not overflow the document.
pub fn offset_to_position(offset: usize, document: &str) -> Position {
    let mut position = Position::default();

    for (i, chr) in document.chars().enumerate() {
        match chr {
            _ if i == offset => {
                return position;
            }
            '\n' => {
                position.character = 0;
                position.line += 1;
            }
            _ => {
                position.character += 1;
            }
        }
    }

    position
}

#[cfg(test)]
mod tests {
    use lsp_types::Position;

    // On Windows, a newline is actually two characters.
    #[test]
    fn position_to_offset_with_crlf() {
        let schema = "\r\nmodel Test {\r\n    id Int @id\r\n}";
        // Let's put the cursor on the "i" in "id Int".
        let expected_offset = schema.chars().position(|c| c == 'i').unwrap();
        let found_offset = super::position_to_offset(
            &Position {
                line: 2,
                character: 4,
            },
            schema,
        )
        .unwrap();

        assert_eq!(found_offset, expected_offset);
    }
}
