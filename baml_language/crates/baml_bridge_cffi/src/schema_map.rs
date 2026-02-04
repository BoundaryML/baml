//! Map VIR schema → bex_program types.
//!
//! This is the mechanical pass-through from VIR's compiler-internal
//! representation to the runtime's BexProgram schema types.

use std::collections::HashMap;

use baml_compiler_vir::{
    VirClass, VirEnum, VirEnumVariant, VirField, VirFunction, VirFunctionBodyKind, VirSchema,
};

/// Map a complete VIR schema to bex_program schema types.
///
/// - Filters `@skip` fields from classes (they don't appear in runtime ClassDef)
/// - Converts `Name` to `String`
/// - Propagates `@alias`, `@description` attributes
/// - Carries `@skip` through on enum variants (runtime decides whether to filter)
/// - Maps function body kinds to `bex_program::FunctionBody`
pub(crate) fn map_schema(
    schema: &VirSchema,
) -> (
    HashMap<String, bex_program::ClassDef>,
    HashMap<String, bex_program::EnumDef>,
    HashMap<String, bex_program::FunctionDef>,
) {
    let classes = schema
        .classes
        .iter()
        .map(|c| (c.name.to_string(), map_class(c)))
        .collect();

    let enums = schema
        .enums
        .iter()
        .map(|e| (e.name.to_string(), map_enum(e)))
        .collect();

    let functions = schema
        .functions
        .iter()
        .map(|f| (f.name.to_string(), map_function(f)))
        .collect();

    (classes, enums, functions)
}

fn map_class(vir: &VirClass) -> bex_program::ClassDef {
    let fields = vir
        .fields
        .iter()
        .filter(|f| !f.skip) // Filter out @skip fields
        .map(map_field)
        .collect();

    bex_program::ClassDef {
        name: vir.name.to_string(),
        fields,
        description: vir.description.clone(),
    }
}

fn map_field(vir: &VirField) -> bex_program::FieldDef {
    bex_program::FieldDef {
        name: vir.name.to_string(),
        field_type: vir.ty.clone(),
        description: vir.description.clone(),
        alias: vir.alias.clone(),
    }
}

fn map_enum(vir: &VirEnum) -> bex_program::EnumDef {
    let variants = vir.variants.iter().map(map_enum_variant).collect();

    bex_program::EnumDef {
        name: vir.name.to_string(),
        variants,
        description: vir.description.clone(),
    }
}

fn map_enum_variant(vir: &VirEnumVariant) -> bex_program::EnumVariantDef {
    bex_program::EnumVariantDef {
        name: vir.name.to_string(),
        description: vir.description.clone(),
        alias: vir.alias.clone(),
        skip: vir.skip,
    }
}

fn map_function(vir: &VirFunction) -> bex_program::FunctionDef {
    let params = vir
        .params
        .iter()
        .map(|p| bex_program::ParamDef {
            name: p.name.to_string(),
            param_type: p.ty.clone(),
        })
        .collect();

    let body = match &vir.body_kind {
        VirFunctionBodyKind::Llm {
            prompt_template,
            client,
        } => bex_program::FunctionBody::Llm {
            prompt_template: prompt_template.clone(),
            client: client.clone(),
        },
        VirFunctionBodyKind::Expr | VirFunctionBodyKind::Missing => bex_program::FunctionBody::Expr,
    };

    bex_program::FunctionDef {
        name: vir.name.to_string(),
        params,
        return_type: vir.return_type.clone(),
        body,
    }
}
