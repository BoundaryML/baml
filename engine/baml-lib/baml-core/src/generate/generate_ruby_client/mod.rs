mod class;
mod client;
mod default_config;
mod r#enum;
mod expression;
mod field_type;
mod function;
mod r#impl;
mod intermediate_repr;
mod ruby_language_features;
mod template;
mod test_case;

use crate::configuration::Generator;

use super::{
    dir_writer::WithFileContentRuby,
    ir::{Expression, IntermediateRepr, WithJsonSchema},
};
use ruby_language_features::{get_file_collector, ToRuby};

pub(crate) fn generate_ruby(ir: &IntermediateRepr, gen: &Generator) -> std::io::Result<()> {
    todo!()
}
