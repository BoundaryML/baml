use anyhow::Result;

use internal_baml_core::ir::{repr::IntermediateRepr, ClassWalker, EnumWalker, FieldType};

#[derive(askama::Template)]
#[template(path = "types.ts.j2", escape = "none")]
pub(crate) struct TypescriptTypes<'ir> {
    enums: Vec<TypescriptEnum<'ir>>,
    classes: Vec<TypescriptClass<'ir>>,
}

struct TypescriptEnum<'ir> {
    pub name: &'ir str,
    pub values: Vec<&'ir str>,
}

struct TypescriptClass<'ir> {
    name: &'ir str,
    fields: Vec<(&'ir str, String)>,
}

impl<'ir> TryFrom<&'ir IntermediateRepr> for TypescriptTypes<'ir> {
    type Error = anyhow::Error;

    fn try_from(ir: &'ir IntermediateRepr) -> Result<TypescriptTypes<'ir>> {
        Ok(TypescriptTypes {
            enums: ir
                .walk_enums()
                .map(|e| Into::<TypescriptEnum>::into(&e))
                .collect::<Vec<_>>(),
            classes: ir
                .walk_classes()
                .map(|e| Into::<TypescriptClass>::into(&e))
                .collect::<Vec<_>>(),
        })
    }
}

impl<'ir> From<&EnumWalker<'ir>> for TypescriptEnum<'ir> {
    fn from(e: &EnumWalker<'ir>) -> TypescriptEnum<'ir> {
        TypescriptEnum {
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

impl<'ir> From<&ClassWalker<'ir>> for TypescriptClass<'ir> {
    fn from(c: &ClassWalker<'ir>) -> TypescriptClass<'ir> {
        TypescriptClass {
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
        super::ToTypeReference::to_type_reference(self)
    }
}
