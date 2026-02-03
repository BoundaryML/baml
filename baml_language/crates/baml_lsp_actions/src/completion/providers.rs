//! Completion providers for different contexts.
//!
//! Each provider generates completions appropriate for a specific context.

use baml_db::{
    baml_compiler_hir::{self, Db, ItemId, file_item_tree, project_items, type_ref_to_str},
    baml_workspace::Project,
};

use super::{CompletionItem, CompletionKind, context::ConfigBlockType};

// ============================================================================
// Helper functions for creating completion items
// ============================================================================

fn keyword(name: &str) -> CompletionItem {
    CompletionItem::new(name, CompletionKind::Keyword).with_sort_text(format!("0{name}"))
}

fn type_item(name: &str) -> CompletionItem {
    CompletionItem::new(name, CompletionKind::Type).with_sort_text(format!("1{name}"))
}

fn property(name: &str) -> CompletionItem {
    CompletionItem::new(name, CompletionKind::Property).with_sort_text(format!("1{name}"))
}

fn snippet(label: &str, insert: &str) -> CompletionItem {
    CompletionItem::new(label, CompletionKind::Snippet)
        .with_insert_text(insert)
        .with_sort_text(format!("0{label}"))
}

fn attr(name: &str) -> CompletionItem {
    CompletionItem::new(name, CompletionKind::Property).with_sort_text(format!("0{name}"))
}

// ============================================================================
// Top-level completions
// ============================================================================

/// Completions for file top-level context.
pub(super) fn complete_top_level() -> Vec<CompletionItem> {
    vec![
        keyword("function").with_detail("Define a function"),
        keyword("class").with_detail("Define a class"),
        keyword("enum").with_detail("Define an enum"),
        keyword("client").with_detail("Define an LLM client"),
        keyword("generator").with_detail("Define a code generator"),
        keyword("test").with_detail("Define a test"),
        keyword("type").with_detail("Define a type alias"),
        keyword("retry_policy").with_detail("Define a retry policy"),
        keyword("template_string").with_detail("Define a template string"),
    ]
}

// ============================================================================
// Type completions
// ============================================================================

/// Completions for type annotation context.
pub(super) fn complete_types(
    db: &dyn Db,
    project: Project,
    partial: Option<&str>,
) -> Vec<CompletionItem> {
    let mut items = vec![
        // Primitive types
        type_item("int").with_detail("Integer type"),
        type_item("float").with_detail("Floating-point type"),
        type_item("string").with_detail("String type"),
        type_item("bool").with_detail("Boolean type"),
        type_item("null").with_detail("Null type"),
        // Media types
        type_item("image").with_detail("Image media type"),
        type_item("audio").with_detail("Audio media type"),
        type_item("video").with_detail("Video media type"),
    ];

    // User-defined types from the project
    let project_items = project_items(db, project);
    for item in project_items.items(db) {
        match item {
            ItemId::Class(loc) => {
                let file = loc.file(db);
                let item_tree = file_item_tree(db, file);
                let class = &item_tree[loc.id(db)];
                let name = class.name.as_str();
                items.push(
                    CompletionItem::new(name, CompletionKind::Class)
                        .with_detail("class")
                        .with_sort_text(format!("2{name}")),
                );
            }
            ItemId::Enum(loc) => {
                let file = loc.file(db);
                let item_tree = file_item_tree(db, file);
                let enum_def = &item_tree[loc.id(db)];
                let name = enum_def.name.as_str();
                items.push(
                    CompletionItem::new(name, CompletionKind::Enum)
                        .with_detail("enum")
                        .with_sort_text(format!("2{name}")),
                );
            }
            ItemId::TypeAlias(loc) => {
                let file = loc.file(db);
                let item_tree = file_item_tree(db, file);
                let alias = &item_tree[loc.id(db)];
                let name = alias.name.as_str();
                items.push(
                    CompletionItem::new(name, CompletionKind::TypeAlias)
                        .with_detail("type alias")
                        .with_sort_text(format!("2{name}")),
                );
            }
            _ => {}
        }
    }

    // Filter by partial if provided
    if let Some(partial) = partial {
        let partial_lower = partial.to_lowercase();
        items.retain(|item| item.label.to_lowercase().starts_with(&partial_lower));
    }

    items
}

// ============================================================================
// Symbol completions
// ============================================================================

/// Completions for symbols (functions, classes, enums, clients).
pub(super) fn complete_symbols(db: &dyn Db, project: Project) -> Vec<CompletionItem> {
    let mut items = vec![];
    let project_items = project_items(db, project);

    for item in project_items.items(db) {
        match item {
            ItemId::Function(loc) => {
                let file = loc.file(db);
                let item_tree = file_item_tree(db, file);
                let func = &item_tree[loc.id(db)];
                let name = func.name.as_str();
                let sig = baml_compiler_hir::function_signature(db, *loc);
                let detail = format_function_signature_short(&sig);
                items.push(
                    CompletionItem::new(name, CompletionKind::Function)
                        .with_detail(detail)
                        .with_sort_text(format!("1{name}")),
                );
            }
            ItemId::Class(loc) => {
                let file = loc.file(db);
                let item_tree = file_item_tree(db, file);
                let class = &item_tree[loc.id(db)];
                let name = class.name.as_str();
                items.push(
                    CompletionItem::new(name, CompletionKind::Class)
                        .with_detail("class")
                        .with_sort_text(format!("2{name}")),
                );
            }
            ItemId::Enum(loc) => {
                let file = loc.file(db);
                let item_tree = file_item_tree(db, file);
                let enum_def = &item_tree[loc.id(db)];
                let name = enum_def.name.as_str();
                items.push(
                    CompletionItem::new(name, CompletionKind::Enum)
                        .with_detail("enum")
                        .with_sort_text(format!("2{name}")),
                );
            }
            ItemId::Client(loc) => {
                let file = loc.file(db);
                let item_tree = file_item_tree(db, file);
                let client = &item_tree[loc.id(db)];
                let name = client.name.as_str();
                items.push(
                    CompletionItem::new(name, CompletionKind::Client)
                        .with_detail("client")
                        .with_sort_text(format!("3{name}")),
                );
            }
            ItemId::Generator(loc) => {
                let file = loc.file(db);
                let item_tree = file_item_tree(db, file);
                let generator = &item_tree[loc.id(db)];
                let name = generator.name.as_str();
                items.push(
                    CompletionItem::new(name, CompletionKind::Generator)
                        .with_detail("generator")
                        .with_sort_text(format!("4{name}")),
                );
            }
            ItemId::Test(loc) => {
                let file = loc.file(db);
                let item_tree = file_item_tree(db, file);
                let test = &item_tree[loc.id(db)];
                let name = test.name.as_str();
                items.push(
                    CompletionItem::new(name, CompletionKind::Test)
                        .with_detail("test")
                        .with_sort_text(format!("5{name}")),
                );
            }
            ItemId::TypeAlias(loc) => {
                let file = loc.file(db);
                let item_tree = file_item_tree(db, file);
                let alias = &item_tree[loc.id(db)];
                let name = alias.name.as_str();
                items.push(
                    CompletionItem::new(name, CompletionKind::TypeAlias)
                        .with_detail("type alias")
                        .with_sort_text(format!("2{name}")),
                );
            }
            ItemId::TemplateString(loc) => {
                let file = loc.file(db);
                let item_tree = file_item_tree(db, file);
                let ts = &item_tree[loc.id(db)];
                let name = ts.name.as_str();
                items.push(
                    CompletionItem::new(name, CompletionKind::TemplateString)
                        .with_detail("template_string")
                        .with_sort_text(format!("1{name}")),
                );
            }
        }
    }

    items
}

/// Format a short function signature for completion detail.
fn format_function_signature_short(sig: &baml_compiler_hir::FunctionSignature) -> String {
    let params: Vec<String> = sig.params.iter().map(|p| p.name.to_string()).collect();
    format!("fn({})", params.join(", "))
}

// ============================================================================
// Field access completions
// ============================================================================

/// Completions after a dot (field access).
pub(super) fn complete_field_access(
    db: &dyn Db,
    project: Project,
    base_text: &str,
) -> Vec<CompletionItem> {
    let mut items = vec![];
    let project_items = project_items(db, project);

    // Look for a class with this name
    for item in project_items.items(db) {
        match item {
            ItemId::Class(loc) => {
                let file = loc.file(db);
                let item_tree = file_item_tree(db, file);
                let class = &item_tree[loc.id(db)];

                if class.name.as_str() == base_text {
                    // Found the class - return its fields
                    for field in &class.fields {
                        let name = field.name.as_str();
                        let type_str = type_ref_to_str(&field.type_ref);
                        items.push(
                            CompletionItem::new(name, CompletionKind::Field)
                                .with_detail(type_str)
                                .with_sort_text(format!("0{name}")),
                        );
                    }
                    return items;
                }
            }
            ItemId::Enum(loc) => {
                let file = loc.file(db);
                let item_tree = file_item_tree(db, file);
                let enum_def = &item_tree[loc.id(db)];

                if enum_def.name.as_str() == base_text {
                    // Found the enum - return its variants
                    for variant in &enum_def.variants {
                        let name = variant.name.as_str();
                        items.push(
                            CompletionItem::new(name, CompletionKind::EnumVariant)
                                .with_detail("variant")
                                .with_sort_text(format!("0{name}")),
                        );
                    }
                    return items;
                }
            }
            _ => {}
        }
    }

    items
}

// ============================================================================
// Prompt context completions
// ============================================================================

/// Completions after `_.` in prompt templates.
pub(super) fn complete_prompt_underscore() -> Vec<CompletionItem> {
    vec![
        snippet("role(\"system\")", "role(\"system\")").with_detail("System role marker"),
        snippet("role(\"user\")", "role(\"user\")").with_detail("User role marker"),
        snippet("role(\"assistant\")", "role(\"assistant\")").with_detail("Assistant role marker"),
    ]
}

/// Completions after `ctx.` in prompt templates.
pub(super) fn complete_prompt_ctx(partial_path: &[String]) -> Vec<CompletionItem> {
    match partial_path {
        [] => {
            // ctx. - show top-level context properties
            vec![
                property("output_format").with_detail("Output format specification"),
                property("client").with_detail("Client configuration"),
            ]
        }
        [s] if s == "client" => {
            // ctx.client. - show client properties
            vec![
                property("name").with_detail("Client name"),
                property("provider").with_detail("Client provider"),
            ]
        }
        _ => vec![],
    }
}

/// Completions for prompt template helpers (outside interpolation).
pub(super) fn complete_prompt_helpers() -> Vec<CompletionItem> {
    vec![
        snippet("{{ }}", "{{ $0 }}").with_detail("Interpolation"),
        snippet("{% for %}", "{% for $1 in $2 %}\n$0\n{% endfor %}").with_detail("For loop"),
        snippet("{% if %}", "{% if $1 %}\n$0\n{% endif %}").with_detail("If conditional"),
        snippet("{# #}", "{# $0 #}").with_detail("Comment"),
    ]
}

// ============================================================================
// Attribute completions
// ============================================================================

/// Completions for field attributes (after @).
pub(super) fn complete_field_attributes(partial: Option<&str>) -> Vec<CompletionItem> {
    let mut items = vec![
        attr("@alias").with_detail("Set an alias name for this field"),
        attr("@description").with_detail("Add a description for this field"),
        attr("@skip").with_detail("Skip this field in serialization"),
        attr("@get").with_detail("Custom getter function"),
        attr("@assert").with_detail("Add an assertion for this field"),
        attr("@check").with_detail("Add a validation check"),
    ];

    // Filter by partial if provided
    if let Some(partial) = partial {
        let partial_lower = partial.to_lowercase();
        items.retain(|item| item.label.to_lowercase().contains(&partial_lower));
    }

    items
}

/// Completions for block attributes (after @@).
pub(super) fn complete_block_attributes(partial: Option<&str>) -> Vec<CompletionItem> {
    let mut items = vec![attr("@@dynamic").with_detail("Mark this type as dynamic")];

    // Filter by partial if provided
    if let Some(partial) = partial {
        let partial_lower = partial.to_lowercase();
        items.retain(|item| item.label.to_lowercase().contains(&partial_lower));
    }

    items
}

// ============================================================================
// Config block completions
// ============================================================================

/// Completions for config blocks.
pub(super) fn complete_config_block(block_type: &ConfigBlockType) -> Vec<CompletionItem> {
    match block_type {
        ConfigBlockType::Client => vec![
            property("provider").with_detail("LLM provider (e.g., openai, anthropic)"),
            property("model").with_detail("Model name"),
            property("api_key").with_detail("API key (use env.*)"),
            property("base_url").with_detail("Custom API base URL"),
            property("temperature").with_detail("Sampling temperature"),
            property("max_tokens").with_detail("Maximum tokens to generate"),
            property("options").with_detail("Additional options"),
        ],
        ConfigBlockType::Generator => vec![
            property("output_type").with_detail("Output type (e.g., python, typescript)"),
            property("output_dir").with_detail("Output directory"),
            property("version").with_detail("Generator version"),
            property("default_client_mode").with_detail("Default client mode"),
        ],
        ConfigBlockType::RetryPolicy => vec![
            property("max_retries").with_detail("Maximum number of retries"),
            property("strategy").with_detail("Retry strategy"),
        ],
        ConfigBlockType::Test => vec![
            property("functions").with_detail("Functions to test"),
            property("args").with_detail("Test arguments"),
        ],
        ConfigBlockType::Unknown => vec![],
    }
}

// ============================================================================
// Expression context completions
// ============================================================================

/// Completions for general expression context.
pub(super) fn complete_expression_context(db: &dyn Db, project: Project) -> Vec<CompletionItem> {
    let mut items = vec![
        // Common expression keywords
        keyword("if").with_detail("If expression"),
        keyword("match").with_detail("Match expression"),
        keyword("for").with_detail("For loop"),
        keyword("let").with_detail("Variable binding"),
        keyword("true").with_detail("Boolean true"),
        keyword("false").with_detail("Boolean false"),
        keyword("null").with_detail("Null value"),
    ];

    // Add symbols
    items.extend(complete_symbols(db, project));

    items
}
