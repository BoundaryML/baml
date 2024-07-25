mod helpers;
mod parse_arguments;
mod parse_attribute;
mod parse_comments;
mod parse_config;
mod parse_expression;
mod parse_field;
mod parse_identifier;
mod parse_named_args_list;
mod parse_schema;
mod parse_template_string;
mod parse_type_expression;
mod parse_types;
mod parse_value_expression;
pub use parse_schema::parse_schema;

// The derive is placed here because it generates the `Rule` enum which is used in all parsing functions.
// It is more convenient if this enum is directly available here.
#[derive(pest_derive::Parser)]
#[grammar = "parser/datamodel.pest"]
pub(crate) struct BAMLParser;
