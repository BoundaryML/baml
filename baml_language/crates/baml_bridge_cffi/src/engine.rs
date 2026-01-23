//! Global BexEngine management.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Result;
use baml_project::ProjectDatabase;
use bex_engine::BexEngine;
use baml_snapshot::BamlSnapshot;
use once_cell::sync::OnceCell;
use tokio::runtime::Runtime;

/// Global BexEngine instance.
static ENGINE: OnceCell<Arc<BexEngine>> = OnceCell::new();

/// Global Tokio runtime for async execution.
static RUNTIME: OnceCell<Arc<Runtime>> = OnceCell::new();

/// Initialize the global Tokio runtime.
pub fn get_runtime() -> &'static Arc<Runtime> {
    RUNTIME.get_or_init(|| {
        Arc::new(Runtime::new().expect("Failed to create Tokio runtime"))
    })
}

/// Get the global BexEngine, or error if not initialized.
pub fn get_engine() -> Result<&'static Arc<BexEngine>> {
    ENGINE
        .get()
        .ok_or_else(|| anyhow::anyhow!("Engine not initialized. Call create_baml_runtime first."))
}

/// Initialize the global BexEngine from BAML source files.
///
/// # Arguments
/// * `root_path` - Root path for BAML files
/// * `src_files` - Map of filename to content
/// * `env_vars` - Environment variables
pub fn initialize_engine(
    root_path: &str,
    src_files: HashMap<String, String>,
    env_vars: HashMap<String, String>,
) -> Result<()> {
    if ENGINE.get().is_some() {
        // Already initialized - this is fine, just return
        return Ok(());
    }

    // Create database
    let mut db = ProjectDatabase::new();

    // Set project root
    let root = Path::new(root_path);
    db.set_project_root(root);

    // Add all source files to the database
    for (filename, content) in src_files {
        let file_path = PathBuf::from(root).join(&filename);
        db.add_or_update_file(&file_path, &content);
    }

    // Compile to bytecode
    let bytecode = baml_compiler_emit::generate_project_bytecode(&db)?;

    // Extract schema information (classes, enums, functions) from the database
    let (classes, enums, functions) = extract_schema(&db)?;

    // Create BamlSnapshot with schema and bytecode
    let snapshot = BamlSnapshot {
        classes,
        enums,
        functions,
        clients: HashMap::new(),
        retry_policies: HashMap::new(),
        bytecode,
    };

    // Create engine
    let engine = BexEngine::new(snapshot, env_vars)?;

    // Store in global
    ENGINE
        .set(Arc::new(engine))
        .map_err(|_| anyhow::anyhow!("Engine already initialized (race condition)"))?;

    Ok(())
}

/// Extract schema information (classes, enums, functions) from the database.
fn extract_schema(
    db: &ProjectDatabase,
) -> Result<(
    HashMap<String, baml_snapshot::ClassDef>,
    HashMap<String, baml_snapshot::EnumDef>,
    HashMap<String, baml_snapshot::FunctionDef>,
)> {
    use baml_compiler_hir::{ItemId, file_item_tree, file_items, function_signature};
    use baml_compiler_tir::TypeResolutionContext;
    use baml_workspace::Db as _;

    let mut classes = HashMap::new();
    let mut enums = HashMap::new();
    let mut functions = HashMap::new();

    let project = db
        .get_project()
        .ok_or_else(|| anyhow::anyhow!("Project not initialized"))?;
    let resolution_ctx = TypeResolutionContext::new(db, project);

    for file in db.get_source_files() {
        let item_tree = file_item_tree(db, file);
        let items_struct = file_items(db, file);

        for item in items_struct.items(db) {
            match item {
                ItemId::Function(func_loc) => {
                    let signature = function_signature(db, *func_loc);

                    // Lower return type from TypeRef to TIR Ty
                    let (tir_return_type, _) = resolution_ctx
                        .lower_type_ref(&signature.return_type, baml_base::Span::default());

                    // Convert TIR Ty to Snapshot Ty
                    let return_type = convert_tir_ty_to_snapshot_ty(&tir_return_type);

                    // Build params
                    let params: Vec<baml_snapshot::ParamDef> = signature
                        .params
                        .iter()
                        .map(|p| {
                            let (tir_ty, _) = resolution_ctx
                                .lower_type_ref(&p.type_ref, baml_base::Span::default());
                            baml_snapshot::ParamDef {
                                name: p.name.to_string(),
                                param_type: convert_tir_ty_to_snapshot_ty(&tir_ty),
                            }
                        })
                        .collect();

                    let func_def = baml_snapshot::FunctionDef {
                        name: signature.name.to_string(),
                        params,
                        return_type,
                        body: baml_snapshot::FunctionBody::Expr {
                            bytecode_index: 0, // Not needed for type checking
                        },
                    };

                    functions.insert(signature.name.to_string(), func_def);
                }
                ItemId::Class(class_loc) => {
                    let class = &item_tree[class_loc.id(db)];
                    let class_name = class.name.to_string();

                    let fields: Vec<baml_snapshot::FieldDef> = class
                        .fields
                        .iter()
                        .map(|field| {
                            let (tir_ty, _) = resolution_ctx
                                .lower_type_ref(&field.type_ref, baml_base::Span::default());
                            baml_snapshot::FieldDef {
                                name: field.name.to_string(),
                                field_type: convert_tir_ty_to_snapshot_ty(&tir_ty),
                                description: None,
                                alias: None,
                            }
                        })
                        .collect();

                    let class_def = baml_snapshot::ClassDef {
                        name: class_name.clone(),
                        fields,
                        description: None,
                    };

                    classes.insert(class_name, class_def);
                }
                ItemId::Enum(enum_loc) => {
                    let enum_def = &item_tree[enum_loc.id(db)];
                    let enum_name = enum_def.name.to_string();

                    let variants: Vec<baml_snapshot::EnumVariantDef> = enum_def
                        .variants
                        .iter()
                        .map(|variant| baml_snapshot::EnumVariantDef {
                            name: variant.name.to_string(),
                            description: None,
                            alias: None,
                        })
                        .collect();

                    let enum_def = baml_snapshot::EnumDef {
                        name: enum_name.clone(),
                        variants,
                        description: None,
                    };

                    enums.insert(enum_name, enum_def);
                }
                _ => {}
            }
        }
    }

    Ok((classes, enums, functions))
}

/// Convert a TIR `Ty` to a Snapshot `Ty`.
fn convert_tir_ty_to_snapshot_ty(tir_ty: &baml_compiler_tir::Ty) -> baml_snapshot::Ty {
    use baml_compiler_tir::Ty as TirTy;
    use baml_snapshot::Ty as SnapTy;

    match tir_ty {
        TirTy::Int => SnapTy::Int,
        TirTy::Float => SnapTy::Float,
        TirTy::String => SnapTy::String,
        TirTy::Bool => SnapTy::Bool,
        TirTy::Null => SnapTy::Null,

        TirTy::Media(kind) => {
            let snap_kind = match kind {
                baml_base::MediaKind::Image => baml_snapshot::MediaKind::Image,
                baml_base::MediaKind::Audio => baml_snapshot::MediaKind::Audio,
                baml_base::MediaKind::Video => baml_snapshot::MediaKind::Video,
                baml_base::MediaKind::Pdf => baml_snapshot::MediaKind::Pdf,
                baml_base::MediaKind::Generic => baml_snapshot::MediaKind::Image,
            };
            SnapTy::Media(snap_kind)
        }

        TirTy::Literal(val) => {
            let snap_val = match val {
                baml_compiler_tir::LiteralValue::Int(i) => baml_snapshot::LiteralValue::Int(*i),
                baml_compiler_tir::LiteralValue::Float(s) => {
                    baml_snapshot::LiteralValue::Int(s.parse().unwrap_or(0))
                }
                baml_compiler_tir::LiteralValue::String(s) => {
                    baml_snapshot::LiteralValue::String(s.clone())
                }
                baml_compiler_tir::LiteralValue::Bool(b) => baml_snapshot::LiteralValue::Bool(*b),
            };
            SnapTy::Literal(snap_val)
        }

        TirTy::Class(fqn) => SnapTy::Class(fqn.to_string()),
        TirTy::Enum(fqn) => SnapTy::Enum(fqn.to_string()),
        TirTy::TypeAlias(fqn) => SnapTy::Class(fqn.to_string()),

        TirTy::Optional(inner) => SnapTy::Optional(Box::new(convert_tir_ty_to_snapshot_ty(inner))),
        TirTy::List(inner) => SnapTy::List(Box::new(convert_tir_ty_to_snapshot_ty(inner))),
        TirTy::Map { key, value } => SnapTy::Map {
            key: Box::new(convert_tir_ty_to_snapshot_ty(key)),
            value: Box::new(convert_tir_ty_to_snapshot_ty(value)),
        },
        TirTy::Union(types) => {
            SnapTy::Union(types.iter().map(convert_tir_ty_to_snapshot_ty).collect())
        }

        TirTy::Function { params, ret } => {
            let _ = (params, ret);
            SnapTy::Null
        }

        TirTy::Unknown | TirTy::Error | TirTy::Void => SnapTy::Null,
        TirTy::WatchAccessor(inner) => convert_tir_ty_to_snapshot_ty(inner),
        TirTy::Builtin(path) => SnapTy::Class(path.clone()),
    }
}
