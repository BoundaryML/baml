//! The Prisma Schema AST.

#![deny(rust_2018_idioms, unsafe_code)]
#![allow(clippy::derive_partial_eq_without_eq)]

pub use self::parser::{parse, parse_standalone_expression, parse_type_builder_contents_from_str};

/// The AST data structure. It aims to faithfully represent the syntax of a Prisma Schema, with
/// source span information.
pub mod ast;

mod formatter;
mod parser;

pub use ast::{diagram_generator, MermaidDiagramGenerator};
pub use formatter::{format_schema, FormatOptions};

/// Transform the input string into a valid (quoted and escaped) PSL string literal.
///
/// PSL string literals have the exact same grammar as [JSON string
/// literals](https://datatracker.ietf.org/doc/html/rfc8259#section-7).
///
/// ```
/// # use internal_baml_ast::string_literal;
///let input = r#"oh
///hi"#;
///assert_eq!(r#""oh\nhi""#, &string_literal(input).to_string());
/// ```
pub fn string_literal(s: &str) -> impl std::fmt::Display + '_ {
    struct StringLiteral<'a>(&'a str);

    impl std::fmt::Display for StringLiteral<'_> {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("\"")?;
            for c in self.0.char_indices() {
                match c {
                    (_, '\t') => f.write_str("\\t")?,
                    (_, '\n') => f.write_str("\\n")?,
                    (_, '"') => f.write_str("\\\"")?,
                    (_, '\r') => f.write_str("\\r")?,
                    (_, '\\') => f.write_str("\\\\")?,
                    // Control characters
                    (_, c) if c.is_ascii_control() => {
                        let mut b = [0];
                        c.encode_utf8(&mut b);
                        f.write_fmt(format_args!("\\u{:04x}", b[0]))?;
                    }
                    (start, other) => f.write_str(&self.0[start..(start + other.len_utf8())])?,
                }
            }
            f.write_str("\"")
        }
    }

    StringLiteral(s)
}
