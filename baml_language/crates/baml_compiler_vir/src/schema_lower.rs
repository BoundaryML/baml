//! Schema lowering: HIR items + TIR types + HIR attributes → `VirSchema`.

use std::collections::{HashMap, HashSet};

use baml_base::{Name, Span};
use baml_compiler_hir::{
    self, Attribute, FunctionBody, ItemId, file_item_tree, file_items, function_body,
    function_signature,
};
use baml_compiler_tir::TypeResolutionContext;
use baml_type::Ty;

use crate::schema::{
    VirClass, VirEnum, VirEnumVariant, VirField, VirFunction, VirFunctionBodyKind, VirParam,
    VirSchema, VirTypeAlias,
};

/// Lower all HIR items in the given project to VIR schema.
///
/// Reads:
/// - HIR items for structure and attributes
/// - TIR `TypeResolutionContext` for resolving `TypeRef` → TIR `Ty`
/// - Type alias maps for non-recursive alias expansion via `baml_type::convert_tir_ty`
pub(crate) fn lower_schema(
    db: &dyn crate::Db,
    project: baml_workspace::Project,
    type_aliases: &HashMap<Name, baml_compiler_tir::Ty>,
    recursive_aliases: &HashSet<Name>,
) -> VirSchema {
    let resolution_ctx = TypeResolutionContext::new(db, project);

    let mut classes = Vec::new();
    let mut enums = Vec::new();
    let mut functions = Vec::new();

    for file in project.files(db) {
        let items_struct = file_items(db, *file);

        for item in items_struct.items(db) {
            match item {
                ItemId::Class(class_loc) => {
                    let item_tree = file_item_tree(db, class_loc.file(db));
                    let class = &item_tree[class_loc.id(db)];
                    classes.push(lower_class(
                        class,
                        &resolution_ctx,
                        type_aliases,
                        recursive_aliases,
                    ));
                }
                ItemId::Enum(enum_loc) => {
                    let item_tree = file_item_tree(db, enum_loc.file(db));
                    let enum_def = &item_tree[enum_loc.id(db)];
                    enums.push(lower_enum(enum_def));
                }
                ItemId::Function(func_loc) => {
                    let signature = function_signature(db, *func_loc);
                    let body = function_body(db, *func_loc);
                    functions.push(lower_function(
                        &signature,
                        &body,
                        &resolution_ctx,
                        type_aliases,
                        recursive_aliases,
                    ));
                }
                _ => {}
            }
        }
    }

    let mut type_alias_defs: Vec<VirTypeAlias> = type_aliases
        .iter()
        .map(|(name, tir_ty)| VirTypeAlias {
            name: name.clone(),
            resolves_to: convert_ty(tir_ty, type_aliases, recursive_aliases),
        })
        .collect();
    type_alias_defs.sort_by(|a, b| a.name.cmp(&b.name));

    VirSchema {
        classes,
        enums,
        functions,
        type_aliases: type_alias_defs,
    }
}

/// Convert an HIR `Attribute<String>` to `Option<String>`.
fn attr_to_option(attr: &Attribute<String>) -> Option<String> {
    attr.value().cloned()
}

/// Convert an HIR `Attribute<()>` to `bool`.
fn attr_to_bool(attr: &Attribute<()>) -> bool {
    attr.is_explicit()
}

/// Convert a TIR type to a runtime-safe `baml_type::Ty`.
fn convert_ty(
    tir_ty: &baml_compiler_tir::Ty,
    type_aliases: &HashMap<Name, baml_compiler_tir::Ty>,
    recursive_aliases: &HashSet<Name>,
) -> Ty {
    baml_type::convert_tir_ty(tir_ty, type_aliases, recursive_aliases)
        .and_then(baml_type::sanitize_for_runtime)
        .unwrap_or(Ty::Null {
            attr: baml_type::TyAttr::default(),
        })
}

fn lower_class(
    class: &baml_compiler_hir::Class,
    resolution_ctx: &TypeResolutionContext,
    type_aliases: &HashMap<Name, baml_compiler_tir::Ty>,
    recursive_aliases: &HashSet<Name>,
) -> VirClass {
    let fields = class
        .fields
        .iter()
        .map(|field| {
            let (tir_ty, _) = resolution_ctx.lower_type_ref(&field.type_ref, Span::default());
            let ty = convert_ty(&tir_ty, type_aliases, recursive_aliases);
            // TyAttr now flows structurally: TypeRef.attr → Ty.attr → baml_type::Ty.attr.
            // No workaround needed.

            VirField {
                name: field.name.clone(),
                ty,
                description: attr_to_option(&field.description),
                alias: attr_to_option(&field.alias),
                skip: attr_to_bool(&field.skip),
                field_attr: field.field_attr.clone(),
            }
        })
        .collect();

    VirClass {
        name: class.name.clone(),
        fields,
        is_dynamic: attr_to_bool(&class.is_dynamic),
        description: attr_to_option(&class.description),
        alias: attr_to_option(&class.alias),
        ty_attr: class.ty_attr.clone(),
    }
}

fn lower_enum(enum_def: &baml_compiler_hir::Enum) -> VirEnum {
    let variants = enum_def
        .variants
        .iter()
        .map(|variant| VirEnumVariant {
            name: variant.name.clone(),
            description: attr_to_option(&variant.description),
            alias: attr_to_option(&variant.alias),
            skip: attr_to_bool(&variant.skip),
        })
        .collect();

    VirEnum {
        name: enum_def.name.clone(),
        variants,
        description: None, // HIR Enum has no @@description
        alias: attr_to_option(&enum_def.alias),
        ty_attr: enum_def.ty_attr.clone(),
    }
}

fn lower_function(
    signature: &baml_compiler_hir::FunctionSignature,
    body: &FunctionBody,
    resolution_ctx: &TypeResolutionContext,
    type_aliases: &HashMap<Name, baml_compiler_tir::Ty>,
    recursive_aliases: &HashSet<Name>,
) -> VirFunction {
    // Lower return type
    let (tir_return_type, _) =
        resolution_ctx.lower_type_ref(&signature.return_type, Span::default());
    let return_type = convert_ty(&tir_return_type, type_aliases, recursive_aliases);

    // Lower params
    let params: Vec<VirParam> = signature
        .params
        .iter()
        .map(|p| {
            let (tir_ty, _) = resolution_ctx.lower_type_ref(&p.type_ref, Span::default());
            VirParam {
                name: p.name.clone(),
                ty: convert_ty(&tir_ty, type_aliases, recursive_aliases),
            }
        })
        .collect();

    // Determine body kind from HIR
    let body_kind = match body {
        FunctionBody::Llm(llm_body) => VirFunctionBodyKind::Llm {
            prompt_template: llm_body.prompt.text.clone(),
            client: llm_body.client.to_string(),
        },
        FunctionBody::Expr(_, _) => VirFunctionBodyKind::Expr,
        FunctionBody::Missing => VirFunctionBodyKind::Missing,
    };

    VirFunction {
        name: signature.name.clone(),
        params,
        return_type,
        body_kind,
    }
}
