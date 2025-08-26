mod helpers;
mod parse;
mod parse_arguments;
mod parse_assignment;
mod parse_attribute;
mod parse_comments;
pub mod parse_expr;
mod parse_expression;
mod parse_field;
mod parse_identifier;
mod parse_named_args_list;
mod parse_template_string;
mod parse_type_builder_block;
mod parse_type_expression_block;
mod parse_types;
mod parse_value_expression_block;
pub use parse::{parse, parse_standalone_expression};
pub use parse_type_builder_block::parse_type_builder_contents_from_str;

// The derive is placed here because it generates the `Rule` enum which is used in all parsing functions.
// It is more convenient if this enum is directly available here.
#[derive(pest_derive::Parser)]
#[grammar = "parser/datamodel.pest"]
pub(crate) struct BAMLParser;
