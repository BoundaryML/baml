//! Completion providers for different contexts.
//!
//! NOTE: HIR-based symbol completions are stubbed — pending compiler2 LSP action reimplementation.
//! Context-independent completions (keywords, attributes, prompt helpers) are fully functional.

use baml_db::{baml_compiler2_hir::Db, baml_workspace::Project};

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
///
/// NOTE: User-defined type completions are stubbed pending compiler2 reimplementation.
#[allow(unused_variables)]
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
///
/// NOTE: Stubbed pending compiler2 reimplementation. Returns empty.
#[allow(unused_variables)]
pub(super) fn complete_symbols(db: &dyn Db, project: Project) -> Vec<CompletionItem> {
    Vec::new()
}

// ============================================================================
// Field access completions
// ============================================================================

/// Completions after a dot (field access).
///
/// NOTE: Stubbed pending compiler2 reimplementation. Returns empty.
#[allow(unused_variables)]
pub(super) fn complete_field_access(
    db: &dyn Db,
    project: Project,
    base_text: &str,
) -> Vec<CompletionItem> {
    Vec::new()
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
            vec![
                property("output_format").with_detail("Output format specification"),
                property("client").with_detail("Client configuration"),
            ]
        }
        [s] if s == "client" => {
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

    if let Some(partial) = partial {
        let partial_lower = partial.to_lowercase();
        items.retain(|item| item.label.to_lowercase().contains(&partial_lower));
    }

    items
}

/// Completions for block attributes (after @@).
pub(super) fn complete_block_attributes(partial: Option<&str>) -> Vec<CompletionItem> {
    let mut items = vec![attr("@@dynamic").with_detail("Mark this type as dynamic")];

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
///
/// NOTE: Symbol completions are stubbed pending compiler2 reimplementation.
#[allow(unused_variables)]
pub(super) fn complete_expression_context(db: &dyn Db, project: Project) -> Vec<CompletionItem> {
    vec![
        keyword("if").with_detail("If expression"),
        keyword("match").with_detail("Match expression"),
        keyword("for").with_detail("For loop"),
        keyword("let").with_detail("Variable binding"),
        keyword("true").with_detail("Boolean true"),
        keyword("false").with_detail("Boolean false"),
        keyword("null").with_detail("Null value"),
    ]
}
