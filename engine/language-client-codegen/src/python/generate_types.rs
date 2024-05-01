use anyhow::Result;
use askama::Template;

use super::python_language_features::ToPython;
use internal_baml_core::ir::{repr::IntermediateRepr, ClassWalker, EnumWalker};

#[derive(askama::Template)]
#[template(path = "types.py.j2", escape = "none")]
pub(crate) struct PythonTypes {
    enums: Vec<String>,
    forward_decls: Vec<String>,
    classes: Vec<String>,
}

#[derive(askama::Template)]
#[template(path = "enum.py.j2")]
struct PythonEnum<'a> {
    pub name: &'a str,
    pub values: Vec<&'a str>,
}

#[derive(askama::Template)]
#[template(path = "class_forward_decl.py.j2")]
struct PythonForwardDecl<'a> {
    name: &'a str,
}

#[derive(askama::Template)]
#[template(path = "class.py.j2")]
struct PythonStruct<'a> {
    name: &'a str,
    fields: Vec<(&'a str, String)>,
}

impl TryFrom<&IntermediateRepr> for PythonTypes {
    type Error = anyhow::Error;

    fn try_from(ir: &IntermediateRepr) -> Result<Self> {
        Ok(PythonTypes {
            enums: ir
                .walk_enums()
                .map(|e| {
                    Into::<PythonEnum>::into(&e)
                        .render()
                        .unwrap_or(format!("# Error rendering enum {}", e.name()))
                })
                .collect(),
            forward_decls: ir
                .walk_classes()
                .map(|c| {
                    PythonForwardDecl { name: c.name() }
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
                    Into::<PythonStruct>::into(&c)
                        .render()
                        .unwrap_or(format!("# Error rendering class {}", c.name()))
                })
                .collect(),
        })
    }
}

impl<'ir> From<&EnumWalker<'ir>> for PythonEnum<'ir> {
    fn from(e: &EnumWalker<'ir>) -> PythonEnum<'ir> {
        PythonEnum {
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

impl<'ir> From<&ClassWalker<'ir>> for PythonStruct<'ir> {
    fn from(c: &ClassWalker<'ir>) -> PythonStruct<'ir> {
        PythonStruct {
            name: c.name(),
            fields: c
                .item
                .elem
                .static_fields
                .iter()
                .map(|f| (f.elem.name.as_str(), f.elem.r#type.elem.to_python()))
                .collect(),
        }
    }
}
