use anyhow::Result;
use askama::Template;

use super::python_language_features::ToPython;
use internal_baml_core::ir::{repr::IntermediateRepr, ClassWalker, EnumWalker};

#[derive(askama::Template)]
#[template(path = "types.py.j2", escape = "none")]
pub(crate) struct PythonTypes<'ir> {
    enums: Vec<PythonEnum<'ir>>,
    classes: Vec<PythonClass<'ir>>,
}

struct PythonEnum<'ir> {
    pub name: &'ir str,
    pub values: Vec<&'ir str>,
}

struct PythonClass<'ir> {
    name: &'ir str,
    fields: Vec<(&'ir str, String)>,
}

impl<'ir> TryFrom<&'ir IntermediateRepr> for PythonTypes<'ir> {
    type Error = anyhow::Error;

    fn try_from(ir: &'ir IntermediateRepr) -> Result<PythonTypes<'ir>> {
        Ok(PythonTypes {
            enums: ir
                .walk_enums()
                .map(|e| Into::<PythonEnum>::into(&e))
                .collect::<Vec<_>>(),
            classes: ir
                .walk_classes()
                .map(|e| Into::<PythonClass>::into(&e))
                .collect::<Vec<_>>(),
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

impl<'ir> From<&ClassWalker<'ir>> for PythonClass<'ir> {
    fn from(c: &ClassWalker<'ir>) -> PythonClass<'ir> {
        PythonClass {
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
