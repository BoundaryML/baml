//! The Prisma Schema AST.

#![deny(rust_2018_idioms, unsafe_code)]
#![allow(clippy::derive_partial_eq_without_eq)]

pub use self::{parser::parse_schema, source_file::SourceFile};

/// The AST data structure. It aims to faithfully represent the syntax of a Prisma Schema, with
/// source span information.
pub mod ast;

mod parser;
mod source_file;

/// Transform the input string into a valid (quoted and escaped) PSL string literal.
///
/// PSL string literals have the exact same grammar as [JSON string
/// literals](https://datatracker.ietf.org/doc/html/rfc8259#section-7).
///
/// ```
/// # use internal_baml_schema_ast::string_literal;
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

// extern crate pest;
// #[macro_use]
// extern crate pest_derive;

// use pest::Parser;
// use std::fs::File;
// use std::io::Read;

// #[derive(Parser)]
// #[grammar = "parser/update.pest"] // Replace with your grammar's path
// pub struct DSLParser;

// pub struct ParsedDSL {
//     // Placeholder for your parsed data
//     // Add more fields as necessary based on what you extract
// }

// pub fn parse_dsl_file(file_path: &str) -> Result<ParsedDSL, String> {
//     let mut file = File::open(file_path).expect("Unable to open file");
//     let mut contents = String::new();
//     file.read_to_string(&mut contents).expect("Unable to read file");

//     let parsed_result = DSLParser::parse(Rule::schema, &contents);
//     match parsed_result {
//         Ok(parsed) => {
//             // TODO: Process the parsed result and populate the ParsedDSL struct.
//             for pair in parsed {
//                 pretty_print(pair, 0);
//             }
//             Ok(ParsedDSL {})
//         }
//         Err(err) => Err(format!("Error parsing file: {}", err)),
//     }
// }

// #[cfg(test)]
// mod tests {
//     use super::*;

//     #[test]
//     fn test_parse_file() {
//         let result = parse_dsl_file("/Users/vbv/repos/gloo-lang/engine/baml/schema-ast/src/parser/example.baml"); // Replace with your test file's path
//         assert!(result.is_ok());
//     }
// }
