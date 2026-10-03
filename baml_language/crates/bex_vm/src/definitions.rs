//! Class and enum definitions ([`sys_types::ClassDefinition`],
//! [`sys_types::EnumDefinition`]) read out of heap declarations.
//!
//! The one reader behind every definition table: the engine's program-wide
//! tables, its per-call runtime-type and runtime-package overlays, and
//! schema-aligned parsing. A class keeps every field with its `skip` flag, and
//! each consumer (output-format rendering, the JSON schema, the parser) leaves
//! skipped fields out itself. An enum leaves out skipped variants, which no
//! consumer reads.

use bex_vm_types::{Class, Enum, RuntimeTy, TyTemplate, TypeHead};
use sys_types::{
    ClassDefinition, ClassFieldDefinition, EnumDefinition, EnumVariantDefinition, SapTy,
    SapTyTemplate,
};

/// The definition of `class`, every field included.
#[must_use]
pub fn class_definition(class: &Class) -> ClassDefinition {
    ClassDefinition {
        name: class.name.display_name().to_string(),
        description: class.description.clone(),
        docstring: class.docstring.clone(),
        alias: class.alias.clone(),
        stream_done: class.stream_done,
        fields: class
            .fields
            .iter()
            .map(|field| ClassFieldDefinition {
                name: field.name.clone(),
                field_type: lane_ty(&field.field_type),
                field_template: Some(lane_template(&field.field_template)),
                description: field.description.clone(),
                docstring: field.docstring.clone(),
                alias: field.alias.clone(),
                skip: field.skip,
                stream_done: field.stream_done,
                must_exist: field.must_exist,
            })
            .collect(),
    }
}

/// The definition of `enm`, without its skipped variants.
#[must_use]
pub fn enum_definition(enm: &Enum) -> EnumDefinition {
    EnumDefinition {
        name: enm.name.display_name().to_string(),
        description: enm.description.clone(),
        docstring: enm.docstring.clone(),
        alias: enm.alias.clone(),
        variants: enm
            .variants
            .iter()
            .filter(|variant| !variant.skip)
            .map(|variant| EnumVariantDefinition {
                name: variant.name.clone(),
                description: variant.description.clone(),
                docstring: variant.docstring.clone(),
                alias: variant.alias.clone(),
            })
            .collect(),
    }
}

/// A declaration's type with each head as the tagged name it identifies.
///
/// Total by invariant: a live declaration's heads are resolved pointers to
/// declarations, so each yields its identity and its own name.
#[must_use]
pub fn lane_ty(ty: &RuntimeTy) -> SapTy {
    ty.try_map_heads(&mut TypeHead::to_tagged_name)
        .unwrap_or_else(|head| {
            unreachable!("a live declaration's field names a declaration: {head}")
        })
}

/// [`lane_ty`] for a field's symbolic template, by the same invariant.
#[must_use]
pub fn lane_template(template: &TyTemplate) -> SapTyTemplate {
    template
        .try_map_heads(&mut TypeHead::to_tagged_name)
        .unwrap_or_else(|head| {
            unreachable!("a live declaration's field template names a declaration: {head}")
        })
}

#[cfg(test)]
mod tests {
    use baml_type::{DeclarationName, Name, typetag::TypeTag};
    use bex_vm_types::{ClassField, EnumVariant, types::Owner};
    use indexmap::IndexMap;

    use super::*;

    fn field(
        name: &str,
        field_type: RuntimeTy,
        field_template: TyTemplate,
        skip: bool,
    ) -> ClassField {
        ClassField {
            name: name.to_string(),
            field_type,
            field_template,
            description: None,
            alias: None,
            docstring: None,
            other: IndexMap::new(),
            skip,
            stream_done: false,
            must_exist: false,
            runtime_type: None,
        }
    }

    fn variant(name: &str, skip: bool) -> EnumVariant {
        EnumVariant {
            name: name.to_string(),
            description: None,
            alias: None,
            docstring: None,
            other: IndexMap::new(),
            skip,
        }
    }

    #[test]
    fn class_definition_keeps_skipped_fields_flagged() {
        let class = Class {
            name: DeclarationName::Anonymous(Name::new("Person")),
            fields: vec![
                field("name", RuntimeTy::String, TyTemplate::String, false),
                field("age", RuntimeTy::Int, TyTemplate::Int, true),
            ],
            description: None,
            alias: None,
            docstring: None,
            other: IndexMap::new(),
            stream_done: false,
            type_tag: TypeTag::fresh_dynamic(),
            has_cleanup: false,
            generic_param_count: 0,
            owner: Owner::anonymous(),
            methods: IndexMap::new(),
        };

        let definition = class_definition(&class);

        let fields: Vec<(&str, bool)> = definition
            .fields
            .iter()
            .map(|field| (field.name.as_str(), field.skip))
            .collect();
        assert_eq!(fields, [("name", false), ("age", true)]);
        assert_eq!(definition.fields[1].field_type, SapTy::Int);
    }

    #[test]
    fn enum_definition_leaves_out_skipped_variants() {
        let enm = Enum {
            name: DeclarationName::Anonymous(Name::new("Color")),
            type_tag: TypeTag::fresh_dynamic(),
            variants: vec![variant("Red", false), variant("Hidden", true)],
            description: None,
            alias: None,
            docstring: None,
            other: IndexMap::new(),
            owner: Owner::anonymous(),
        };

        let definition = enum_definition(&enm);

        let names: Vec<&str> = definition
            .variants
            .iter()
            .map(|variant| variant.name.as_str())
            .collect();
        assert_eq!(names, ["Red"]);
    }
}
