use anyhow::Result;
use askama::Template;

use super::golang_language_features::ToGolang;
use internal_baml_core::ir::{repr::IntermediateRepr, ClassWalker, EnumWalker};

#[derive(askama::Template)]
#[template(path = "types.rb.j2", escape = "none")]
pub(crate) struct GolangTypes {
    enums: Vec<String>,
    forward_decls: Vec<String>,
    classes: Vec<String>,
}

#[derive(askama::Template)]
#[template(path = "enum.rb.j2")]
struct GolangEnum<'a> {
    pub name: &'a str,
    pub values: Vec<&'a str>,
}

#[derive(askama::Template)]
#[template(path = "class_forward_decl.rb.j2")]
struct GolangForwardDecl<'a> {
    name: &'a str,
}

#[derive(askama::Template)]
#[template(path = "class.rb.j2")]
struct GolangStruct<'a> {
    name: &'a str,
    fields: Vec<(&'a str, String)>,
}

impl TryFrom<&IntermediateRepr> for GolangTypes {
    type Error = anyhow::Error;

    fn try_from(ir: &IntermediateRepr) -> Result<Self> {
        Ok(GolangTypes {
            enums: ir
                .walk_enums()
                .map(|e| {
                    Into::<GolangEnum>::into(&e)
                        .render()
                        .unwrap_or(format!("# Error rendering enum {}", e.name()))
                })
                .collect(),
            forward_decls: ir
                .walk_classes()
                .map(|c| {
                    GolangForwardDecl { name: c.name() }
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
                    Into::<GolangStruct>::into(&c)
                        .render()
                        .unwrap_or(format!("# Error rendering class {}", c.name()))
                })
                .collect(),
        })
    }
}

impl<'ir> From<&EnumWalker<'ir>> for GolangEnum<'ir> {
    fn from(e: &EnumWalker<'ir>) -> GolangEnum<'ir> {
        GolangEnum {
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

impl<'ir> From<&ClassWalker<'ir>> for GolangStruct<'ir> {
    fn from(c: &ClassWalker<'ir>) -> GolangStruct<'ir> {
        GolangStruct {
            name: c.name(),
            fields: c
                .item
                .elem
                .static_fields
                .iter()
                .map(|f| (f.elem.name.as_str(), f.elem.r#type.elem.to_golang()))
                .collect(),
        }
    }
}
