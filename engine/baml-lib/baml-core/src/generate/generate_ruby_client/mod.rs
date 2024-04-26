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

use askama::Template;

use crate::configuration::Generator;

use super::{
    dir_writer::WithFileContentRuby,
    ir::{Expression, IntermediateRepr, WithJsonSchema},
};
use ruby_language_features::{get_file_collector, ToRuby};

#[derive(Template)]
#[template(path = "enum.rb.j2")]
struct RubyEnum<'a> {
    name: &'a str,
    values: Vec<&'a str>,
}

#[derive(askama::Template)]
#[template(path = "class.rb.j2")]
struct RubyStruct<'a> {
    name: &'a str,
    fields: Vec<(&'a str, String)>,
}

pub(crate) fn generate_ruby(ir: &IntermediateRepr, gen: &Generator) -> std::io::Result<()> {
    let mut collector = get_file_collector();

    let file = collector.start_file(".", "types", false);
    ir.walk_enums().for_each(|e| {
        file.append(
            RubyEnum {
                name: e.name(),
                values: e
                    .item
                    .elem
                    .values
                    .iter()
                    .map(|v| v.elem.0.as_str())
                    .collect(),
            }
            .render()
            .unwrap_or("# Error rendering enum".to_string()),
        );
    });
    ir.walk_classes().for_each(|c| {
        file.append(
            RubyStruct {
                name: c.name(),
                fields: c
                    .item
                    .elem
                    .static_fields
                    .iter()
                    .map(|f| (f.elem.name.as_str(), f.elem.r#type.elem.to_ruby()))
                    .collect(),
            }
            .render()
            .unwrap_or("# Error rendering enum".to_string()),
        );
    });
    collector.finish_file();

    ir.write(&mut collector);

    collector.commit(&gen.output_path)
}
