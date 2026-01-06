use std::path::PathBuf;

use baml_db::{RootDatabase, SourceFile, baml_hir, baml_tir, baml_workspace};
use wasm_bindgen::prelude::*;

#[cfg(feature = "console_error_panic")]
extern crate console_error_panic_hook;

#[cfg(feature = "small_allocator")]
#[global_allocator]
static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;

pub mod sam_sandbox;

#[wasm_bindgen(start)]
pub fn start() {
    #[cfg(feature = "console_error_panic")]
    console_error_panic_hook::set_once();
}

/// A basic runtime wrapper around BAML source content.
#[wasm_bindgen]
pub struct BamlRuntime {
    baml_src: String,
    db: RootDatabase,
    project: baml_workspace::Project,
    source_file: Option<SourceFile>,
}

#[wasm_bindgen]
impl BamlRuntime {
    #[wasm_bindgen(constructor)]
    pub fn new(baml_src: String) -> BamlRuntime {
        use baml_db::Setter;

        let mut db = RootDatabase::new();

        // Create a project with a virtual file path
        let project = db.set_project_root(PathBuf::from("/baml_src"));

        // Add the source file to the database
        let source_file = db.add_file("/baml_src/main.baml", &baml_src);

        // Wire up the project to include this file
        project.set_files(&mut db).to(vec![source_file]);

        BamlRuntime {
            baml_src,
            db,
            project,
            source_file: Some(source_file),
        }
    }

    /// Renders the stored BAML source into a set of naming-case variants.
    #[wasm_bindgen]
    pub fn render(&self) -> sam_sandbox::CasingVariants {
        sam_sandbox::CasingVariants::new(&self.baml_src)
    }

    /// Allows updating the stored BAML source for subsequent renders.
    ///
    /// This uses Salsa's incremental computation - only queries affected
    /// by the text change will be recomputed on subsequent calls.
    #[wasm_bindgen]
    pub fn set_source(&mut self, baml_src: String) {
        use baml_db::Setter;

        self.baml_src = baml_src.clone();

        // Update the source file in the Salsa database
        // This marks dependent queries as potentially stale
        if let Some(source_file) = self.source_file {
            source_file.set_text(&mut self.db).to(baml_src);
        }
    }

    /// Returns the names of all functions defined in the BAML project.
    ///
    /// This uses Salsa's `project_function_names` tracked query, which:
    /// 1. Depends on `project_items` → `file_items` → `file_item_tree`
    /// 2. Is memoized - subsequent calls return cached results if source unchanged
    /// 3. Only recomputes when function signatures change (not body edits)
    #[wasm_bindgen]
    pub fn function_names(&self) -> Vec<String> {
        let names_struct = baml_hir::project_function_names(&self.db, self.project);
        let mut foo = names_struct.names(&self.db).clone();
        foo.push("injected-hot-reload4".to_string());
        foo
    }

    /// Convenience helper returning the raw BAML source currently stored.
    #[wasm_bindgen(getter)]
    pub fn baml_src(&self) -> String {
        self.baml_src.clone()
    }

    /// Get the typed body of a function, including type inference results.
    ///
    /// Returns a `FunctionTypedBodyResult` containing:
    /// - The pretty-printed typed IR tree
    /// - Any type errors found during inference
    /// - Function metadata (signature, body kind)
    #[wasm_bindgen]
    pub fn get_function_typed_body(&self, function_name: &str) -> FunctionTypedBodyResult {
        // Step 1: Find the function by name
        let func_loc = match self.find_function_by_name(function_name) {
            Some(loc) => loc,
            None => {
                return FunctionTypedBodyResult {
                    success: false,
                    error: Some(format!("Function '{}' not found", function_name)),
                    tree: None,
                    type_errors: vec![],
                    signature: None,
                    body_kind: None,
                };
            }
        };

        // Step 2: Get signature and body from HIR
        let signature = baml_hir::function_signature(&self.db, func_loc);
        let body = baml_hir::function_body(&self.db, func_loc);

        // Step 3: Determine body kind
        let body_kind = match body.as_ref() {
            baml_hir::FunctionBody::Llm(_) => "llm",
            baml_hir::FunctionBody::Expr(_) => "expr",
            baml_hir::FunctionBody::Missing => "missing",
        };

        // Step 4: Build typing context
        let globals = baml_tir::typing_context(&self.db, self.project);
        let class_fields = baml_tir::class_field_types(&self.db, self.project);
        let type_aliases = baml_tir::type_aliases(&self.db, self.project);
        let enum_variants_map = baml_tir::enum_variants(&self.db, self.project);
        let enum_variants = enum_variants_map.enums(&self.db).clone();

        // Step 5: Run type inference
        let inference_result = baml_tir::infer_function(
            &self.db,
            &signature,
            &body,
            Some(globals),
            Some(class_fields),
            Some(type_aliases),
            Some(enum_variants),
            func_loc,
        );

        // Step 6: Render the tree
        let tree = baml_tir::render_function_tree(
            &self.db,
            function_name,
            &signature,
            &body,
            &inference_result,
        );

        // Step 7: Format type errors
        let type_errors: Vec<String> = inference_result
            .errors
            .iter()
            .map(baml_tir::short_display)
            .collect();

        // Step 8: Format signature
        let signature_str = format_signature(&signature);

        FunctionTypedBodyResult {
            success: true,
            error: None,
            tree: Some(tree),
            type_errors,
            signature: Some(signature_str),
            body_kind: Some(body_kind.to_string()),
        }
    }
}

impl BamlRuntime {
    /// Find a FunctionLoc by name, iterating through project items.
    fn find_function_by_name(&self, name: &str) -> Option<baml_hir::FunctionLoc<'_>> {
        let items = baml_hir::project_items(&self.db, self.project);

        for item in items.items(&self.db) {
            if let baml_hir::ItemId::Function(func_loc) = item {
                let file = func_loc.file(&self.db);
                let item_tree = baml_hir::file_item_tree(&self.db, file);
                let func = &item_tree[func_loc.id(&self.db)];
                if func.name.as_str() == name {
                    return Some(*func_loc);
                }
            }
        }
        None
    }
}

/// Format a function signature as a string.
fn format_signature(sig: &baml_hir::FunctionSignature) -> String {
    let params: Vec<String> = sig
        .params
        .iter()
        .map(|p| format!("{}: {:?}", p.name, p.type_ref))
        .collect();
    format!("{}({}) -> {:?}", sig.name, params.join(", "), sig.return_type)
}

/// Result of getting a function's typed body.
#[wasm_bindgen]
pub struct FunctionTypedBodyResult {
    success: bool,
    error: Option<String>,
    tree: Option<String>,
    type_errors: Vec<String>,
    signature: Option<String>,
    body_kind: Option<String>,
}

#[wasm_bindgen]
impl FunctionTypedBodyResult {
    #[wasm_bindgen(getter)]
    pub fn success(&self) -> bool {
        self.success
    }

    #[wasm_bindgen(getter)]
    pub fn error(&self) -> Option<String> {
        self.error.clone()
    }

    #[wasm_bindgen(getter)]
    pub fn tree(&self) -> Option<String> {
        self.tree.clone()
    }

    #[wasm_bindgen(getter)]
    pub fn type_errors(&self) -> Vec<String> {
        self.type_errors.clone()
    }

    #[wasm_bindgen(getter)]
    pub fn signature(&self) -> Option<String> {
        self.signature.clone()
    }

    #[wasm_bindgen(getter)]
    pub fn body_kind(&self) -> Option<String> {
        self.body_kind.clone()
    }
}
