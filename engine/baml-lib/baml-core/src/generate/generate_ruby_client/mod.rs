mod expression;
mod field_type;
mod ruby_language_features;

use askama::Template;
use either::Either;

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
#[template(path = "class_forward_decl.rb.j2")]
struct RubyForwardDecl<'a> {
    name: &'a str,
    fields: Vec<(&'a str, String)>,
}

#[derive(askama::Template)]
#[template(path = "class.rb.j2")]
struct RubyStruct<'a> {
    name: &'a str,
    fields: Vec<(&'a str, String)>,
}

#[derive(askama::Template)]
#[template(path = "types.rb.j2", escape = "none")]
struct RubyTypes {
    enums: Vec<String>,
    forward_decls: Vec<String>,
    classes: Vec<String>,
}
#[derive(askama::Template)]
#[template(path = "client.rb.j2", escape = "none")]
struct RubyClient {
    funcs: Vec<RubyFunction>,
}
struct RubyFunction {
    name: String,
    return_type: String,
    args: Vec<(String, String)>,
}

pub(crate) fn generate_ruby(ir: &IntermediateRepr, gen: &Generator) -> std::io::Result<()> {
    let mut collector = get_file_collector();

    let file = collector.start_file(".", "types", false);
    file.append(
        RubyTypes {
            enums: ir
                .walk_enums()
                .map(|e| {
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
                    .unwrap_or(format!("# Error rendering enum {}", e.name()))
                })
                .collect(),
            forward_decls: ir
                .walk_classes()
                .map(|c| {
                    RubyForwardDecl {
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
                    .unwrap_or(format!("# Error rendering class {}", c.name()))
                })
                .collect(),
            classes: ir
                .walk_classes()
                .map(|c| {
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
                    .unwrap_or(format!("# Error rendering class {}", c.name()))
                })
                .collect(),
        }
        .render()
        .unwrap_or("# Error rendering types".to_string()),
    );
    collector.finish_file();

    let file = collector.start_file(".", "client", false);
    let functions = ir
        .walk_functions()
        .flat_map(|f| {
            let Either::Right(configs) = f.walk_impls() else {
                return vec![];
            };
            let funcs = configs
                .map(|c| {
                    let (function, impl_) = c.item;
                    RubyFunction {
                        name: f.name().to_string(),
                        return_type: f.elem().output().to_ruby(),
                        args: match f.inputs() {
                            // TODO: unnamed args should fail explicitly instead of silently
                            either::Either::Left(args) => vec![],
                            either::Either::Right(args) => args
                                .iter()
                                .map(|(name, r#type)| (name.to_string(), r#type.to_ruby()))
                                .collect(),
                        },
                    }
                })
                .collect::<Vec<_>>();
            funcs
        })
        .collect();
    file.append(
        RubyClient { funcs: functions }
            .render()
            .unwrap_or("# Error rendering client".to_string()),
    );
    collector.finish_file();

    collector.commit(&gen.output_path)
}
