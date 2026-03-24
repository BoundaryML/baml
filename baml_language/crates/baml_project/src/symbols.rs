//! Symbol listing and lookup for BAML projects.
//!
//! This module provides APIs for listing symbols (functions, classes, enums, etc.)
//! in a BAML project.

pub use baml_db::baml_compiler_hir::SymbolKind;
use baml_db::{
    Name, Span,
    baml_compiler_hir::{self, Db, ItemId, file_item_tree, file_namespace, project_items},
    baml_compiler_parser::syntax_tree,
    baml_compiler_syntax::ast::{FunctionDef, Item, SourceFile},
    baml_workspace::Project,
};
use rowan::ast::AstNode;
use text_size::TextRange;

/// Information about a symbol in the project.
#[derive(Debug, Clone)]
pub struct Symbol {
    /// The name of the symbol.
    pub name: String,
    /// The kind of symbol.
    pub kind: SymbolKind,
    /// The file path containing the symbol.
    pub file_path: std::path::PathBuf,
    /// The span of the symbol in the source.
    pub span: Span,
}

/// List all functions in the project.
pub fn list_functions(db: &dyn Db, project: Project) -> Vec<Symbol> {
    let func_list = baml_compiler_hir::list_function_names(db, project);

    func_list
        .into_iter()
        .filter_map(|(func_name, span)| {
            let file = project
                .files(db)
                .iter()
                .find(|f| f.file_id(db) == span.file_id)?;
            let file_path = file.path(db);

            Some(Symbol {
                name: func_name,
                kind: SymbolKind::Function,
                file_path,
                span,
            })
        })
        .collect()
}

/// Extended function metadata for the playground.
#[derive(Debug, Clone)]
pub struct FunctionSymbol {
    pub name: String,
    /// Whether this is an LLM function (has LLM body in CST).
    pub is_llm: bool,
    /// The LLM client name (if LLM function).
    pub client_name: Option<String>,
    /// Whether this function is compiler-generated (render_prompt, build_request, resolve).
    pub is_sub_function: bool,
}

/// List user-facing functions with metadata for the playground.
///
/// Filters out compiler-generated sub-functions (render_prompt, build_request, resolve)
/// and returns structured metadata per function.
pub fn list_functions_with_metadata(db: &dyn Db, project: Project) -> Vec<FunctionSymbol> {
    let items = project_items(db, project);
    let mut functions = Vec::new();

    for item in items.items(db) {
        if let ItemId::Function(func_loc) = item {
            let file = func_loc.file(db);
            let item_tree = file_item_tree(db, file);
            let func = &item_tree[func_loc.id(db)];

            // Skip functions from builtin files (e.g., <builtin>/baml/llm.baml)
            if file_namespace(db, file).is_some() {
                continue;
            }

            // Skip sub-functions — they become capabilities on the parent
            match &func.compiler_generated {
                Some(baml_compiler_hir::CompilerGenerated::LlmRenderPrompt { .. })
                | Some(baml_compiler_hir::CompilerGenerated::LlmBuildRequest { .. })
                | Some(baml_compiler_hir::CompilerGenerated::ClientResolve { .. }) => {
                    continue;
                }
                _ => {}
            }

            // Helper to get function defs from the CST for this file
            let get_fn_defs = || -> Vec<FunctionDef> {
                let tree = syntax_tree(db, file);
                let source_file = SourceFile::cast(tree).unwrap();
                source_file
                    .items()
                    .filter_map(|i: Item| match i {
                        Item::Function(f) => Some(f),
                        _ => None,
                    })
                    .collect()
            };

            // Determine if LLM function:
            // - LlmCall compiler-generated → is_llm = true
            // - User-defined (None) → check CST for LLM body
            let is_llm = match &func.compiler_generated {
                Some(baml_compiler_hir::CompilerGenerated::LlmCall { .. }) => true,
                None => get_fn_defs().into_iter().any(|f: FunctionDef| {
                    f.name().as_ref().map(|n| n.text()) == Some(func.name.as_str())
                        && f.llm_body().is_some()
                }),
                _ => false,
            };

            // Get client name for LLM functions from CST
            let client_name = if is_llm {
                get_fn_defs()
                    .into_iter()
                    .find(|f: &FunctionDef| {
                        f.name().as_ref().map(|n| n.text()) == Some(func.name.as_str())
                    })
                    .and_then(|f: FunctionDef| f.llm_body())
                    .and_then(|body| body.client_field())
                    .and_then(|c| c.value())
            } else {
                None
            };

            functions.push(FunctionSymbol {
                name: func.name.to_string(),
                is_llm,
                client_name,
                is_sub_function: false,
            });
        }
    }

    functions
}

/// Extended test metadata for the playground.
#[derive(Debug, Clone)]
pub struct TestSymbol {
    pub name: String,
    /// The first function this test targets (from function_refs[0]).
    pub function_name: String,
    /// Test args serialized as a JSON string.
    pub args_json: String,
}

/// Convert a TestArgValue to serde_json::Value for serialization.
fn test_arg_to_json(val: &baml_compiler_hir::TestArgValue) -> serde_json::Value {
    match val {
        baml_compiler_hir::TestArgValue::Int(n) => serde_json::Value::Number((*n).into()),
        baml_compiler_hir::TestArgValue::Float(f) => serde_json::Number::from_f64(*f)
            .map(serde_json::Value::Number)
            .unwrap_or(serde_json::Value::Null),
        baml_compiler_hir::TestArgValue::Bool(b) => serde_json::Value::Bool(*b),
        baml_compiler_hir::TestArgValue::String(s) => serde_json::Value::String(s.clone()),
        baml_compiler_hir::TestArgValue::Null => serde_json::Value::Null,
        baml_compiler_hir::TestArgValue::Array(arr) => {
            serde_json::Value::Array(arr.iter().map(test_arg_to_json).collect())
        }
        baml_compiler_hir::TestArgValue::Map(map) => serde_json::Value::Object(
            map.iter()
                .map(|(k, v)| (k.clone(), test_arg_to_json(v)))
                .collect(),
        ),
    }
}

/// List tests with full metadata for the playground.
///
/// Filters out tests from builtin files and returns structured metadata
/// including the parent function name and serialized args.
pub fn list_tests_with_metadata(db: &dyn Db, project: Project) -> Vec<TestSymbol> {
    let items = project_items(db, project);
    let mut tests = Vec::new();

    for item in items.items(db) {
        if let ItemId::Test(test_loc) = item {
            let file = test_loc.file(db);

            // Skip tests from builtin files
            if file_namespace(db, file).is_some() {
                continue;
            }

            let item_tree = file_item_tree(db, file);
            let test = &item_tree[test_loc.id(db)];

            // Skip tests with no function references
            let function_name = match test.function_refs.first() {
                Some(name) => name.to_string(),
                None => continue,
            };

            // Serialize test args to JSON
            let args_obj: serde_json::Map<String, serde_json::Value> = test
                .args
                .iter()
                .map(|(k, v)| (k.clone(), test_arg_to_json(v)))
                .collect();
            let args_json = serde_json::to_string(&serde_json::Value::Object(args_obj))
                .unwrap_or_else(|_| "{}".to_string());

            tests.push(TestSymbol {
                name: test.name.to_string(),
                function_name,
                args_json,
            });
        }
    }

    tests
}

/// List all classes in the project.
pub fn list_classes(db: &dyn Db, project: Project) -> Vec<Symbol> {
    let mut classes = Vec::new();
    let items = project_items(db, project);

    for item in items.items(db) {
        if let ItemId::Class(class_loc) = item {
            let file = class_loc.file(db);
            let item_tree = file_item_tree(db, file);
            let class = &item_tree[class_loc.id(db)];

            let file_path = file.path(db);
            let span = Span::new(file.file_id(db), TextRange::empty(0.into()));

            classes.push(Symbol {
                name: class.name.to_string(),
                kind: SymbolKind::Class,
                file_path,
                span,
            });
        }
    }

    classes
}

/// List all enums in the project.
pub fn list_enums(db: &dyn Db, project: Project) -> Vec<Symbol> {
    let mut enums = Vec::new();
    let items = project_items(db, project);

    for item in items.items(db) {
        if let ItemId::Enum(enum_loc) = item {
            let file = enum_loc.file(db);
            let item_tree = file_item_tree(db, file);
            let enum_def = &item_tree[enum_loc.id(db)];

            let file_path = file.path(db);
            let span = Span::new(file.file_id(db), TextRange::empty(0.into()));

            enums.push(Symbol {
                name: enum_def.name.to_string(),
                kind: SymbolKind::Enum,
                file_path,
                span,
            });
        }
    }

    enums
}

/// List all type aliases in the project.
pub fn list_type_aliases(db: &dyn Db, project: Project) -> Vec<Symbol> {
    let mut aliases = Vec::new();
    let items = project_items(db, project);

    for item in items.items(db) {
        if let ItemId::TypeAlias(alias_loc) = item {
            let file = alias_loc.file(db);
            let item_tree = file_item_tree(db, file);
            let alias = &item_tree[alias_loc.id(db)];

            let file_path = file.path(db);
            let span = Span::new(file.file_id(db), TextRange::empty(0.into()));

            aliases.push(Symbol {
                name: alias.name.to_string(),
                kind: SymbolKind::TypeAlias,
                file_path,
                span,
            });
        }
    }

    aliases
}

/// List all clients in the project.
pub fn list_clients(db: &dyn Db, project: Project) -> Vec<Symbol> {
    let mut clients = Vec::new();
    let items = project_items(db, project);

    for item in items.items(db) {
        if let ItemId::Client(client_loc) = item {
            let file = client_loc.file(db);
            let item_tree = file_item_tree(db, file);
            let client = &item_tree[client_loc.id(db)];

            let file_path = file.path(db);
            let span = Span::new(file.file_id(db), TextRange::empty(0.into()));

            clients.push(Symbol {
                name: client.name.to_string(),
                kind: SymbolKind::Client,
                file_path,
                span,
            });
        }
    }

    clients
}

/// List all tests in the project.
pub fn list_tests(db: &dyn Db, project: Project) -> Vec<Symbol> {
    let mut tests = Vec::new();
    let items = project_items(db, project);

    for item in items.items(db) {
        if let ItemId::Test(test_loc) = item {
            let file = test_loc.file(db);
            let item_tree = file_item_tree(db, file);
            let test = &item_tree[test_loc.id(db)];

            let file_path = file.path(db);
            let span = Span::new(file.file_id(db), TextRange::empty(0.into()));

            tests.push(Symbol {
                name: test.name.to_string(),
                kind: SymbolKind::Test,
                file_path,
                span,
            });
        }
    }

    tests
}

/// List all generators in the project.
pub fn list_generators(db: &dyn Db, project: Project) -> Vec<Symbol> {
    let mut generators = Vec::new();
    let items = project_items(db, project);

    for item in items.items(db) {
        if let ItemId::Generator(gen_loc) = item {
            let file = gen_loc.file(db);
            let item_tree = file_item_tree(db, file);
            let generator = &item_tree[gen_loc.id(db)];

            let file_path = file.path(db);
            let span = Span::new(file.file_id(db), TextRange::empty(0.into()));

            generators.push(Symbol {
                name: generator.name.to_string(),
                kind: SymbolKind::Generator,
                file_path,
                span,
            });
        }
    }

    generators
}

/// Find a symbol by name in the project.
///
/// Returns the first matching symbol, or None if not found.
pub fn find_symbol(db: &dyn Db, project: Project, name: &str) -> Option<Symbol> {
    find_symbol_locations(db, project, name).into_iter().next()
}

/// Find all locations where a symbol with the given name is defined.
///
/// This supports multi-file definitions (e.g., partial classes spread across files).
pub fn find_symbol_locations(db: &dyn Db, project: Project, name: &str) -> Vec<Symbol> {
    let mut locations = Vec::new();
    let name_to_find = Name::new(name);
    let items = project_items(db, project);

    for item in items.items(db) {
        if let Some(symbol) = item_to_symbol(db, *item, &name_to_find) {
            locations.push(symbol);
        }
    }

    locations
}

/// Convert an `ItemId` to a Symbol if it matches the given name.
fn item_to_symbol(db: &dyn Db, item: ItemId<'_>, name_to_find: &Name) -> Option<Symbol> {
    match item {
        ItemId::Function(func_loc) => {
            let file = func_loc.file(db);
            let item_tree = file_item_tree(db, file);
            let func = &item_tree[func_loc.id(db)];

            if &func.name == name_to_find {
                let file_path = file.path(db);
                let span = Span::new(file.file_id(db), TextRange::empty(0.into()));

                Some(Symbol {
                    name: func.name.to_string(),
                    kind: SymbolKind::Function,
                    file_path,
                    span,
                })
            } else {
                None
            }
        }
        ItemId::Class(class_loc) => {
            let file = class_loc.file(db);
            let item_tree = file_item_tree(db, file);
            let class = &item_tree[class_loc.id(db)];

            if &class.name == name_to_find {
                let file_path = file.path(db);
                let span = Span::new(file.file_id(db), TextRange::empty(0.into()));

                Some(Symbol {
                    name: class.name.to_string(),
                    kind: SymbolKind::Class,
                    file_path,
                    span,
                })
            } else {
                // Also check class fields
                for field in &class.fields {
                    if &field.name == name_to_find {
                        let file_path = file.path(db);
                        let span = Span::new(file.file_id(db), TextRange::empty(0.into()));

                        return Some(Symbol {
                            name: field.name.to_string(),
                            kind: SymbolKind::Field,
                            file_path,
                            span,
                        });
                    }
                }
                None
            }
        }
        ItemId::Enum(enum_loc) => {
            let file = enum_loc.file(db);
            let item_tree = file_item_tree(db, file);
            let enum_def = &item_tree[enum_loc.id(db)];

            if &enum_def.name == name_to_find {
                let file_path = file.path(db);
                let span = Span::new(file.file_id(db), TextRange::empty(0.into()));

                Some(Symbol {
                    name: enum_def.name.to_string(),
                    kind: SymbolKind::Enum,
                    file_path,
                    span,
                })
            } else {
                // Also check enum variants
                for variant in &enum_def.variants {
                    if &variant.name == name_to_find {
                        let file_path = file.path(db);
                        let span = Span::new(file.file_id(db), TextRange::empty(0.into()));

                        return Some(Symbol {
                            name: variant.name.to_string(),
                            kind: SymbolKind::EnumVariant,
                            file_path,
                            span,
                        });
                    }
                }
                None
            }
        }
        ItemId::TypeAlias(alias_loc) => {
            let file = alias_loc.file(db);
            let item_tree = file_item_tree(db, file);
            let alias = &item_tree[alias_loc.id(db)];

            if &alias.name == name_to_find {
                let file_path = file.path(db);
                let span = Span::new(file.file_id(db), TextRange::empty(0.into()));

                Some(Symbol {
                    name: alias.name.to_string(),
                    kind: SymbolKind::TypeAlias,
                    file_path,
                    span,
                })
            } else {
                None
            }
        }
        ItemId::Client(client_loc) => {
            let file = client_loc.file(db);
            let item_tree = file_item_tree(db, file);
            let client = &item_tree[client_loc.id(db)];

            if &client.name == name_to_find {
                let file_path = file.path(db);
                let span = Span::new(file.file_id(db), TextRange::empty(0.into()));

                Some(Symbol {
                    name: client.name.to_string(),
                    kind: SymbolKind::Client,
                    file_path,
                    span,
                })
            } else {
                None
            }
        }
        ItemId::Test(test_loc) => {
            let file = test_loc.file(db);
            let item_tree = file_item_tree(db, file);
            let test = &item_tree[test_loc.id(db)];

            if &test.name == name_to_find {
                let file_path = file.path(db);
                let span = Span::new(file.file_id(db), TextRange::empty(0.into()));

                Some(Symbol {
                    name: test.name.to_string(),
                    kind: SymbolKind::Test,
                    file_path,
                    span,
                })
            } else {
                None
            }
        }
        ItemId::Generator(gen_loc) => {
            let file = gen_loc.file(db);
            let item_tree = file_item_tree(db, file);
            let generator = &item_tree[gen_loc.id(db)];

            if &generator.name == name_to_find {
                let file_path = file.path(db);
                let span = Span::new(file.file_id(db), TextRange::empty(0.into()));

                Some(Symbol {
                    name: generator.name.to_string(),
                    kind: SymbolKind::Generator,
                    file_path,
                    span,
                })
            } else {
                None
            }
        }
        ItemId::TemplateString(ts_loc) => {
            let file = ts_loc.file(db);
            let item_tree = file_item_tree(db, file);
            let ts = &item_tree[ts_loc.id(db)];

            if &ts.name == name_to_find {
                let file_path = file.path(db);
                let span = Span::new(file.file_id(db), TextRange::empty(0.into()));

                Some(Symbol {
                    name: ts.name.to_string(),
                    kind: SymbolKind::TemplateString,
                    file_path,
                    span,
                })
            } else {
                None
            }
        }
        ItemId::RetryPolicy(rp_loc) => {
            let file = rp_loc.file(db);
            let item_tree = file_item_tree(db, file);
            let rp = &item_tree[rp_loc.id(db)];

            if &rp.name == name_to_find {
                let file_path = file.path(db);
                let span = Span::new(file.file_id(db), TextRange::empty(0.into()));

                Some(Symbol {
                    name: rp.name.to_string(),
                    kind: SymbolKind::RetryPolicy,
                    file_path,
                    span,
                })
            } else {
                None
            }
        }
    }
}
