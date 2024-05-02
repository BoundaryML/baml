use anyhow::Result;
use askama::Template;

use super::python_language_features::ToPython;
use internal_baml_core::ir::{repr::IntermediateRepr, ClassWalker, EnumWalker, FieldType};

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
                .map(|f| (f.elem.name.as_str(), f.elem.r#type.elem.to_type_decl()))
                .collect(),
        }
    }
}

trait ToTypeDeclaration {
    fn to_type_decl(&self) -> String;
}

impl ToTypeDeclaration for FieldType {
    fn to_type_decl(&self) -> String {
        match self {
            FieldType::Class(name) | FieldType::Enum(name) => format!("\"{name}\""),
            FieldType::List(inner) => format!("List[{}]", inner.to_type_decl()),
            FieldType::Map(key, value) => {
                format!("Dict[{}, {}]", key.to_type_decl(), value.to_type_decl())
            }
            FieldType::Primitive(r#type) => r#type.to_python(),
            FieldType::Union(inner) => format!(
                "Union[{}]",
                inner
                    .iter()
                    .map(|t| t.to_type_decl())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            FieldType::Tuple(inner) => format!(
                "Tuple[{}]",
                inner
                    .iter()
                    .map(|t| t.to_type_decl())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            FieldType::Optional(inner) => format!("Optional[{}]", inner.to_type_decl()),
        }
    }
}
