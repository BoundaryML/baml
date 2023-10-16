mod helpers;
mod parse_comments;
mod parse_enum;
mod parse_class;
mod parse_schema;
mod parse_field;
mod parse_types;
mod parse_expression;
mod parse_attribute;
mod parse_arguments;
mod parse_function;
mod parse_client;

pub use parse_schema::parse_schema;

// The derive is placed here because it generates the `Rule` enum which is used in all parsing functions.
// It is more convenient if this enum is directly available here.
#[derive(pest_derive::Parser)]
#[grammar = "parser/datamodel.pest"]
pub(crate) struct BAMLParser;
