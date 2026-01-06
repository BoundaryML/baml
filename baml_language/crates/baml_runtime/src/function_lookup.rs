//! Function lookup utilities using TIR.
//!
//! Provides functions to look up BAML function definitions, prompts, and clients
//! from a compiled database using the Typed Intermediate Representation.

use std::sync::Arc;

use baml_db::{baml_workspace::Project, RootDatabase, SourceFile};
use baml_hir::{self, file_item_tree, FunctionBody, FunctionLoc, FunctionSignature, ItemId};
use baml_tir::{Ty, TypeResolutionContext};

/// Information about an LLM function's body.
#[derive(Debug, Clone)]
pub struct LlmFunctionInfo {
    /// The prompt template text.
    pub prompt: Option<String>,
    /// The client name.
    pub client: Option<String>,
}

/// Information about a function including its signature and body.
#[derive(Debug, Clone)]
pub struct FunctionInfo {
    /// The function name.
    pub name: String,
    /// The function signature with resolved types.
    pub signature: ResolvedSignature,
    /// The body kind and details.
    pub body: FunctionBodyInfo,
}

/// A function signature with resolved TIR types.
#[derive(Debug, Clone)]
pub struct ResolvedSignature {
    /// Function name.
    pub name: String,
    /// Parameters with resolved types.
    pub params: Vec<ResolvedParam>,
    /// Return type.
    pub return_type: Ty,
}

/// A parameter with resolved type.
#[derive(Debug, Clone)]
pub struct ResolvedParam {
    /// Parameter name.
    pub name: String,
    /// Resolved type.
    pub ty: Ty,
}

/// Information about a function's body.
#[derive(Debug, Clone)]
pub enum FunctionBodyInfo {
    /// LLM function with prompt template.
    Llm(LlmFunctionInfo),
    /// Expression-based function.
    Expr,
    /// Missing body.
    Missing,
}

/// Find a function by name in a project.
///
/// Returns the FunctionLoc if found.
pub fn find_function_by_name<'db>(
    db: &'db RootDatabase,
    project: Project,
    name: &str,
) -> Option<FunctionLoc<'db>> {
    let items = baml_hir::project_items(db, project);

    for item in items.items(db) {
        if let ItemId::Function(func_loc) = item {
            let file = func_loc.file(db);
            let item_tree = file_item_tree(db, file);
            let func = &item_tree[func_loc.id(db)];
            if func.name.as_str() == name {
                return Some(*func_loc);
            }
        }
    }
    None
}

/// Find a function by name in a single source file.
pub fn find_function_in_file<'db>(
    db: &'db RootDatabase,
    source: SourceFile,
    name: &str,
) -> Option<FunctionLoc<'db>> {
    let item_tree = file_item_tree(db, source);
    let items = baml_hir::file_items(db, source);

    for item in items.items(db) {
        if let ItemId::Function(func_loc) = item {
            let func = &item_tree[func_loc.id(db)];
            if func.name.as_str() == name {
                return Some(*func_loc);
            }
        }
    }
    None
}

/// Get the first function in a source file.
pub fn get_first_function<'db>(db: &'db RootDatabase, source: SourceFile) -> Option<FunctionLoc<'db>> {
    let items = baml_hir::file_items(db, source);

    for item in items.items(db) {
        if let ItemId::Function(func_loc) = item {
            return Some(*func_loc);
        }
    }
    None
}

/// Get the first function name in a source file.
pub fn get_first_function_name(db: &RootDatabase, source: SourceFile) -> Option<String> {
    let item_tree = file_item_tree(db, source);
    let items = baml_hir::file_items(db, source);

    for item in items.items(db) {
        if let ItemId::Function(func_loc) = item {
            let func = &item_tree[func_loc.id(db)];
            return Some(func.name.to_string());
        }
    }
    None
}

/// Get the prompt template from a function.
///
/// Returns None if the function is not an LLM function or has no prompt.
pub fn get_function_prompt(db: &RootDatabase, func_loc: FunctionLoc<'_>) -> Option<String> {
    let body = baml_hir::function_body(db, func_loc);
    if let FunctionBody::Llm(llm) = body.as_ref() {
        if let Some(ref prompt) = llm.prompt {
            return Some(prompt.text.clone());
        }
    }
    None
}

/// Get the client name from a function.
///
/// Returns None if the function is not an LLM function or has no client.
pub fn get_function_client(db: &RootDatabase, func_loc: FunctionLoc<'_>) -> Option<String> {
    let body = baml_hir::function_body(db, func_loc);
    if let FunctionBody::Llm(llm) = body.as_ref() {
        if let Some(ref client) = llm.client {
            return Some(client.to_string());
        }
    }
    None
}

/// Get the function signature from a function location.
pub fn get_function_signature(db: &RootDatabase, func_loc: FunctionLoc<'_>) -> Arc<FunctionSignature> {
    baml_hir::function_signature(db, func_loc)
}

/// Get the function body from a function location.
pub fn get_function_body(db: &RootDatabase, func_loc: FunctionLoc<'_>) -> std::sync::Arc<FunctionBody> {
    baml_hir::function_body(db, func_loc)
}

/// Get complete function info with resolved types.
pub fn get_function_info(
    db: &RootDatabase,
    project: Project,
    func_loc: FunctionLoc<'_>,
) -> FunctionInfo {
    let file = func_loc.file(db);
    let item_tree = file_item_tree(db, file);
    let func = &item_tree[func_loc.id(db)];
    let signature = baml_hir::function_signature(db, func_loc);
    let body = baml_hir::function_body(db, func_loc);

    // Create type resolution context
    let resolution_ctx = TypeResolutionContext::new(db, project);

    // Resolve parameter types
    let params = signature
        .params
        .iter()
        .map(|p| {
            let (ty, _errors) = resolution_ctx.lower_type_ref(&p.type_ref, Default::default());
            ResolvedParam {
                name: p.name.to_string(),
                ty,
            }
        })
        .collect();

    // Resolve return type
    let (return_type, _errors) = resolution_ctx.lower_type_ref(&signature.return_type, Default::default());

    let resolved_signature = ResolvedSignature {
        name: func.name.to_string(),
        params,
        return_type,
    };

    // Get body info
    let body_info = match body.as_ref() {
        FunctionBody::Llm(llm) => FunctionBodyInfo::Llm(LlmFunctionInfo {
            prompt: llm.prompt.as_ref().map(|p| p.text.clone()),
            client: llm.client.clone().map(|c| c.to_string()),
        }),
        FunctionBody::Expr(_) => FunctionBodyInfo::Expr,
        FunctionBody::Missing => FunctionBodyInfo::Missing,
    };

    FunctionInfo {
        name: func.name.to_string(),
        signature: resolved_signature,
        body: body_info,
    }
}

/// List all function names in a project.
pub fn list_function_names(db: &RootDatabase, project: Project) -> Vec<String> {
    let names = baml_hir::project_function_names(db, project);
    names.names(db).clone()
}

/// List all function names in a source file.
pub fn list_function_names_in_file(db: &RootDatabase, source: SourceFile) -> Vec<String> {
    let item_tree = file_item_tree(db, source);
    let items = baml_hir::file_items(db, source);

    let mut names = Vec::new();
    for item in items.items(db) {
        if let ItemId::Function(func_loc) = item {
            let func = &item_tree[func_loc.id(db)];
            names.push(func.name.to_string());
        }
    }
    names
}
