use std::borrow::Cow;

use anyhow::Result;

use internal_baml_core::ir::{repr::IntermediateRepr, ClassWalker, EnumWalker};

use crate::{type_check_attributes, GeneratorArgs, TypeCheckAttributes};

use super::ToTypeReferenceInClientDefinition;

#[derive(askama::Template)]
#[template(path = "type_builder.ts.j2", escape = "none")]
pub(crate) struct TypeBuilder<'ir> {
    enums: Vec<TypescriptEnum<'ir>>,
    classes: Vec<TypescriptClass<'ir>>,
}

#[derive(askama::Template)]
#[template(path = "types.ts.j2", escape = "none")]
pub(crate) struct TypescriptTypes<'ir> {
    enums: Vec<TypescriptEnum<'ir>>,
    check_classes: Vec<TypescriptClass<'ir>>,
    classes: Vec<TypescriptClass<'ir>>,
}

struct TypescriptEnum<'ir> {
    pub name: &'ir str,
    pub values: Vec<&'ir str>,
    pub dynamic: bool,
}

pub struct TypescriptClass<'ir> {
    pub name: Cow<'ir, str>,
    pub fields: Vec<(Cow<'ir, str>, bool, String)>,
    pub dynamic: bool,
}

// TODO: Use this.
// pub struct TypescriptChecksClass {
//     pub name: String,
//     pub fields: Vec<(String, bool, String)>,
//     pub dynamic: bool,
// }

impl<'ir> TryFrom<(&'ir IntermediateRepr, &'ir GeneratorArgs)> for TypescriptTypes<'ir> {
    type Error = anyhow::Error;

    fn try_from(
        (ir, _): (&'ir IntermediateRepr, &'ir GeneratorArgs),
    ) -> Result<TypescriptTypes<'ir>> {
        Ok(TypescriptTypes {
            enums: ir
                .walk_enums()
                .map(|e| Into::<TypescriptEnum>::into(&e))
                .collect::<Vec<_>>(),
            check_classes: type_check_attributes(ir)
                .iter()
                .map(|checks| type_def_for_checks(checks))
                .collect::<Vec<_>>(),
            classes: ir
                .walk_classes()
                .map(|e| Into::<TypescriptClass>::into(&e))
                .collect::<Vec<_>>(),
        })
    }
}

impl<'ir> TryFrom<(&'ir IntermediateRepr, &'ir GeneratorArgs)> for TypeBuilder<'ir> {
    type Error = anyhow::Error;

    fn try_from((ir, _): (&'ir IntermediateRepr, &'ir GeneratorArgs)) -> Result<TypeBuilder<'ir>> {
        Ok(TypeBuilder {
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
            dynamic: e.item.attributes.get("dynamic_type").is_some(),
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
            name: Cow::Borrowed(c.name()),
            dynamic: c.item.attributes.get("dynamic_type").is_some(),
            fields: c
                .item
                .elem
                .static_fields
                .iter()
                .map(|f| {
                    (
                        Cow::Borrowed(f.elem.name.as_str()),
                        f.elem.r#type.elem.is_optional(),
                        f.elem.r#type.elem.to_type_ref(&c.db),
                    )
                })
                .collect(),
        }
    }
}

pub fn type_def_for_checks(checks: &TypeCheckAttributes) -> TypescriptClass<'static> {
    TypescriptClass {
        name: Cow::Owned(type_name_for_checks(checks)),
        dynamic: false,
        fields: checks.0.iter().map(|check_name| (Cow::Owned(check_name.clone()), false, "Check".to_string())).collect(),
    }
}

pub fn type_name_for_checks(checks: &TypeCheckAttributes) -> String {
    let mut name = "Checks".to_string();
    let mut names: Vec<&String> = checks.0.iter().collect();
    names.sort();
    for check_name in names.iter() {
        name.push_str("__");
        name.push_str(check_name);
    }
    name
}
