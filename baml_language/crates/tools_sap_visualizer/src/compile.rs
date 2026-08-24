//! Compiles arbitrary BAML source + a type expression into SAP model types.
//!
//! Concatenates user BAML code with a synthetic class containing a field of the
//! target type, compiles via the BAML pipeline, then extracts class/enum
//! definitions from the bytecode to build a `TypeRefDb` and `AnnotatedTy` for
//! the SAP visualizer.

use std::sync::Arc;

use bex_sap::sap_model::{self, AnnotatedTy, TypeRefDb};
use indexmap::IndexMap;
use ouroboros::self_referencing;

/// The synthetic class name injected into the BAML source.
const PARSE_CLASS: &str = "__SapParseTarget";
/// The synthetic field name within the class.
const PARSE_FIELD: &str = "value";

/// Self-referential struct that owns the `TypeCtx` and the lane type
/// for the parse target, and borrows `TypeRefDb` + `AnnotatedTy` from them.
#[self_referencing]
pub struct CompiledSapModel {
    type_ctx: sap_model::TypeCtx,
    /// The lane type extracted from the synthetic class field.
    parse_ty: ::sys_types::SapTy,
    #[borrows(type_ctx)]
    #[covariant]
    pub db: TypeRefDb<'this, ::sys_types::DefKey>,
    #[borrows(type_ctx, parse_ty)]
    #[covariant]
    pub ty: AnnotatedTy<'this, ::sys_types::DefKey>,
}

impl CompiledSapModel {
    /// Access db and ty together within a closure (ouroboros makes individual
    /// field accessors private).
    pub fn with_db_and_ty<R>(
        &self,
        f: impl for<'this> FnOnce(
            &'this TypeRefDb<'this, ::sys_types::DefKey>,
            &'this AnnotatedTy<'this, ::sys_types::DefKey>,
        ) -> R,
    ) -> R {
        self.with(|fields| f(fields.db, fields.ty))
    }
}

/// Compile BAML source + type expression into a SAP model.
///
/// `baml_source` is arbitrary BAML code (class/enum definitions, etc.).
/// `type_expr` is a BAML type expression like `Resume`, `string | int`, `Foo[]`, etc.
///
/// Returns `Err` with an error message on compilation failure.
pub fn compile_baml_to_sap(baml_source: &str, type_expr: &str) -> Result<CompiledSapModel, String> {
    if type_expr.trim().is_empty() {
        return Err("Type expression is empty".to_string());
    }

    // Synthesize a class with a single field of the target type.
    // We use a class field rather than a type alias because non-recursive
    // type aliases are expanded inline and don't survive into the bytecode.
    let synthetic_source =
        format!("{baml_source}\n\nclass {PARSE_CLASS} {{\n  {PARSE_FIELD} {type_expr}\n}}\n");

    // Set up the compiler database.
    let mut db = baml_project::ProjectDatabase::new();
    db.set_project_root(std::path::Path::new("/tmp/sap_visualizer"));
    db.add_or_update_file(
        std::path::Path::new("/tmp/sap_visualizer/main.baml"),
        &synthetic_source,
    );

    // Compile to bytecode.
    let program = db
        .get_bytecode()
        .map_err(|e| format!("Compilation error: {e}"))?;

    // Extract class and enum definitions from the compiled object pool.
    // This mirrors `BexEngine::extract_class_definitions` / `extract_enum_definitions`.
    let mut class_defs: IndexMap<::sys_types::DefKey, sys_types::ClassDefinition> = IndexMap::new();
    let mut enum_defs: IndexMap<::sys_types::DefKey, sys_types::EnumDefinition> = IndexMap::new();
    let mut parse_field_ty: Option<::sys_types::SapTy> = None;

    for obj in &program.objects {
        match obj {
            // A compiled pool's declarations are all declared, so the
            // qualified name is always present.
            bex_vm_types::Object::Class(cls)
                if cls
                    .name
                    .declared()
                    .is_some_and(|qtn| qtn.package().as_str() != "baml") =>
            {
                let qtn = cls
                    .name
                    .declared()
                    .cloned()
                    .unwrap_or_else(|| unreachable!("guarded by declared() above"));
                // Extract the field type from our synthetic class.
                if qtn.name().as_str() == PARSE_CLASS {
                    let field = cls
                        .fields
                        .iter()
                        .find(|f| f.name == PARSE_FIELD)
                        .ok_or_else(|| {
                            format!("Synthetic class {PARSE_CLASS} missing field {PARSE_FIELD}")
                        })?;
                    parse_field_ty = Some(
                        field
                            .field_type
                            .try_map_heads(&mut bex_vm_types::TypeHead::to_tagged_name)
                            .map_err(|head| {
                                format!(
                                    "Synthetic class {PARSE_CLASS} field {PARSE_FIELD} has an \
                                     unnameable head: {head}"
                                )
                            })?,
                    );
                    // Don't add the synthetic class to the definitions.
                    continue;
                }

                class_defs.insert(
                    ::sys_types::DefKey::new(cls.type_tag, cls.name.clone()),
                    sys_types::ClassDefinition {
                        name: cls.name.display_name().to_string(),
                        description: cls.description.clone(),
                        alias: cls.alias.clone(),
                        fields: cls
                            .fields
                            .iter()
                            .map(|f| sys_types::ClassFieldDefinition {
                                name: f.name.clone(),
                                field_type: f
                                    .field_type
                                    .try_map_heads(&mut bex_vm_types::TypeHead::to_tagged_name)
                                    .unwrap_or_else(|_| ::sys_types::SapTy::unknown()),
                                field_template: f
                                    .field_template
                                    .try_map_heads(&mut bex_vm_types::TypeHead::to_tagged_name)
                                    .ok(),
                                description: f.description.clone(),
                                alias: f.alias.clone(),
                                skip: f.skip,
                            })
                            .collect(),
                    },
                );
            }
            bex_vm_types::Object::Enum(enm)
                if enm
                    .name
                    .declared()
                    .is_some_and(|qtn| qtn.package().as_str() != "baml") =>
            {
                enum_defs.insert(
                    ::sys_types::DefKey::new(enm.type_tag, enm.name.clone()),
                    sys_types::EnumDefinition {
                        name: enm.name.display_name().to_string(),
                        description: enm.description.clone(),
                        alias: enm.alias.clone(),
                        variants: enm
                            .variants
                            .iter()
                            .filter(|v| !v.skip)
                            .map(|v| sys_types::EnumVariantDefinition {
                                name: v.name.clone(),
                                description: v.description.clone(),
                                alias: v.alias.clone(),
                            })
                            .collect(),
                    },
                );
            }
            _ => {}
        }
    }

    let parse_ty = parse_field_ty
        .ok_or_else(|| format!("Synthetic class {PARSE_CLASS} not found in compiled output"))?;

    let type_alias_definitions = program
        .recursive_type_aliases()
        .into_iter()
        .filter_map(|(head, ty)| {
            ty.try_map_heads(&mut bex_vm_types::TypeHead::to_tagged_name)
                .ok()
                .map(|ty| (head, ::sys_types::SapTy::from(ty)))
        })
        .collect();
    let type_ctx =
        sap_model::TypeCtx::new(&class_defs, Arc::new(enum_defs), &type_alias_definitions);

    CompiledSapModelTryBuilder {
        type_ctx,
        parse_ty,
        db_builder: |type_ctx: &sap_model::TypeCtx| {
            type_ctx
                .build_db()
                .map_err(|e| format!("SAP type conversion error: {e}"))
        },
        ty_builder: |type_ctx: &sap_model::TypeCtx, parse_ty: &::sys_types::SapTy| {
            type_ctx
                .convert_ty(parse_ty)
                .map_err(|e| format!("Failed to convert parse type: {e}"))
        },
    }
    .try_build()
}
