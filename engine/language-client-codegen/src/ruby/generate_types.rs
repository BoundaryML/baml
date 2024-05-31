use anyhow::Result;
use askama::Template;

use super::ruby_language_features::ToRuby;
use internal_baml_core::ir::{repr::IntermediateRepr, ClassWalker, EnumWalker};

#[derive(askama::Template)]
#[template(path = "types.rb.j2", escape = "none")]
pub(crate) struct RubyTypes {
    enums: Vec<String>,
    forward_decls: Vec<String>,
    classes: Vec<String>,
}

#[derive(askama::Template)]
#[template(path = "enum.rb.j2")]
struct RubyEnum<'a> {
    pub name: &'a str,
    pub values: Vec<&'a str>,
}

#[derive(askama::Template)]
#[template(path = "class_forward_decl.rb.j2")]
struct RubyForwardDecl<'a> {
    name: &'a str,
}

#[derive(askama::Template)]
#[template(path = "class.rb.j2")]
struct RubyStruct<'a> {
    name: &'a str,
    fields: Vec<(&'a str, String)>,
}

impl TryFrom<&IntermediateRepr> for RubyTypes {
    type Error = anyhow::Error;

    fn try_from(ir: &IntermediateRepr) -> Result<Self> {
        Ok(RubyTypes {
            enums: ir
                .walk_enums()
                .map(|e| {
                    Into::<RubyEnum>::into(&e)
                        .render()
                        .unwrap_or(format!("# Error rendering enum {}", e.name()))
                })
                .collect(),
            forward_decls: ir
                .walk_classes()
                .map(|c| {
                    RubyForwardDecl { name: c.name() }
                        .render()
                        .unwrap_or(format!(
                            "# Error rendering forward decl for class {}",
                            c.name()
                        ))
                })
                .collect(),
            classes: ir
                .walk_classes()
                .map(|c| {
                    Into::<RubyStruct>::into(&c)
                        .render()
                        .unwrap_or(format!("# Error rendering class {}", c.name()))
                })
                .collect(),
        })
    }
}

impl<'ir> From<&EnumWalker<'ir>> for RubyEnum<'ir> {
    fn from(e: &EnumWalker<'ir>) -> RubyEnum<'ir> {
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
    }
}

impl<'ir> From<&ClassWalker<'ir>> for RubyStruct<'ir> {
    fn from(c: &ClassWalker<'ir>) -> RubyStruct<'ir> {
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
    }
}
