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

        // Step 6: Create resolution context for rendering
        let resolution_ctx = baml_tir::TypeResolutionContext::new(&self.db, self.project);

        // Step 7: Render the tree
        let tree = baml_tir::render_function_tree(
            &self.db,
            &resolution_ctx,
            function_name,
            &signature,
            &body,
            &inference_result,
        );

        // Step 8: Format type errors
        let type_errors: Vec<String> = inference_result
            .errors
            .iter()
            .map(baml_tir::short_display)
            .collect();

        // Step 9: Format signature
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

// ============================================================================
// Runtime Execution Bindings
// ============================================================================

/// Result from rendering a prompt.
#[wasm_bindgen]
pub struct RenderPromptResult {
    success: bool,
    error: Option<String>,
    prompt: Option<String>,
    messages_json: Option<String>,
}

#[wasm_bindgen]
impl RenderPromptResult {
    #[wasm_bindgen(getter)]
    pub fn success(&self) -> bool {
        self.success
    }

    #[wasm_bindgen(getter)]
    pub fn error(&self) -> Option<String> {
        self.error.clone()
    }

    #[wasm_bindgen(getter)]
    pub fn prompt(&self) -> Option<String> {
        self.prompt.clone()
    }

    #[wasm_bindgen(getter)]
    pub fn messages_json(&self) -> Option<String> {
        self.messages_json.clone()
    }
}

/// Result from rendering a curl command.
#[wasm_bindgen]
pub struct RenderCurlResult {
    success: bool,
    error: Option<String>,
    curl: Option<String>,
}

#[wasm_bindgen]
impl RenderCurlResult {
    #[wasm_bindgen(getter)]
    pub fn success(&self) -> bool {
        self.success
    }

    #[wasm_bindgen(getter)]
    pub fn error(&self) -> Option<String> {
        self.error.clone()
    }

    #[wasm_bindgen(getter)]
    pub fn curl(&self) -> Option<String> {
        self.curl.clone()
    }
}

/// Result from building a request.
#[wasm_bindgen]
pub struct BuildRequestResult {
    success: bool,
    error: Option<String>,
    url: Option<String>,
    method: Option<String>,
    headers_json: Option<String>,
    body_json: Option<String>,
}

#[wasm_bindgen]
impl BuildRequestResult {
    #[wasm_bindgen(getter)]
    pub fn success(&self) -> bool {
        self.success
    }

    #[wasm_bindgen(getter)]
    pub fn error(&self) -> Option<String> {
        self.error.clone()
    }

    #[wasm_bindgen(getter)]
    pub fn url(&self) -> Option<String> {
        self.url.clone()
    }

    #[wasm_bindgen(getter)]
    pub fn method(&self) -> Option<String> {
        self.method.clone()
    }

    #[wasm_bindgen(getter)]
    pub fn headers_json(&self) -> Option<String> {
        self.headers_json.clone()
    }

    #[wasm_bindgen(getter)]
    pub fn body_json(&self) -> Option<String> {
        self.body_json.clone()
    }
}

#[wasm_bindgen]
impl BamlRuntime {
    /// Render a prompt for a function without executing.
    ///
    /// Takes a function name and JSON-encoded arguments.
    #[wasm_bindgen]
    pub fn render_prompt_for_function(
        &self,
        function_name: &str,
        args_json: &str,
    ) -> RenderPromptResult {
        // Parse arguments
        let args: baml_runtime::BamlMap<String, baml_runtime::BamlValue> =
            match serde_json::from_str(args_json) {
                Ok(args) => args,
                Err(e) => {
                    return RenderPromptResult {
                        success: false,
                        error: Some(format!("Failed to parse arguments: {}", e)),
                        prompt: None,
                        messages_json: None,
                    };
                }
            };

        // Create a stub prepared function
        // TODO: Get actual prompt template from function definition
        let prepared = baml_runtime::PreparedFunction::new_stub(
            function_name,
            args,
            baml_runtime::TypeRef::string(),
            baml_runtime::ClientSpec::new("openai/gpt-4"),
            baml_runtime::PromptTemplate::new("{{ input }} and this is more"),
        );

        // Render the prompt
        match baml_runtime::render_prompt(&prepared) {
            Ok(prompt) => {
                let text = prompt
                    .messages
                    .iter()
                    .map(|m| m.text_content())
                    .collect::<Vec<_>>()
                    .join("\n");

                // Serialize messages to JSON for structured access
                let messages: Vec<serde_json::Value> = prompt
                    .messages
                    .iter()
                    .map(|m| {
                        serde_json::json!({
                            "role": m.role.as_str(),
                            "content": m.text_content()
                        })
                    })
                    .collect();

                RenderPromptResult {
                    success: true,
                    error: None,
                    prompt: Some(text),
                    messages_json: serde_json::to_string(&messages).ok(),
                }
            }
            Err(e) => RenderPromptResult {
                success: false,
                error: Some(e.to_string()),
                prompt: None,
                messages_json: None,
            },
        }
    }

    /// Generate a curl command for a function.
    ///
    /// Takes a function name, JSON-encoded arguments, and whether to expose secrets.
    #[wasm_bindgen]
    pub fn render_curl_for_function(
        &self,
        function_name: &str,
        args_json: &str,
        expose_secrets: bool,
    ) -> RenderCurlResult {
        // Parse arguments
        let args: baml_runtime::BamlMap<String, baml_runtime::BamlValue> =
            match serde_json::from_str(args_json) {
                Ok(args) => args,
                Err(e) => {
                    return RenderCurlResult {
                        success: false,
                        error: Some(format!("Failed to parse arguments: {}", e)),
                        curl: None,
                    };
                }
            };

        // Create a stub prepared function
        let prepared = baml_runtime::PreparedFunction::new_stub(
            function_name,
            args,
            baml_runtime::TypeRef::string(),
            baml_runtime::ClientSpec::new("openai/gpt-4"),
            baml_runtime::PromptTemplate::new("{{ input }}"),
        );

        // Create context
        let ctx = baml_runtime::context::PerCallContext::new();

        // Create render options
        let options = if expose_secrets {
            baml_runtime::RenderOptions::for_execution()
        } else {
            baml_runtime::RenderOptions::default()
        };

        // Render the curl command
        match baml_runtime::render_raw_curl(&prepared, &ctx, &options) {
            Ok(curl) => RenderCurlResult {
                success: true,
                error: None,
                curl: Some(curl),
            },
            Err(e) => RenderCurlResult {
                success: false,
                error: Some(e.to_string()),
                curl: None,
            },
        }
    }

    /// Build a provider-specific request for a function.
    ///
    /// Returns the request details as structured data.
    #[wasm_bindgen]
    pub fn build_request_for_function(
        &self,
        function_name: &str,
        args_json: &str,
        stream: bool,
    ) -> BuildRequestResult {
        // Parse arguments
        let args: baml_runtime::BamlMap<String, baml_runtime::BamlValue> =
            match serde_json::from_str(args_json) {
                Ok(args) => args,
                Err(e) => {
                    return BuildRequestResult {
                        success: false,
                        error: Some(format!("Failed to parse arguments: {}", e)),
                        url: None,
                        method: None,
                        headers_json: None,
                        body_json: None,
                    };
                }
            };

        // Create a stub prepared function
        let prepared = baml_runtime::PreparedFunction::new_stub(
            function_name,
            args,
            baml_runtime::TypeRef::string(),
            baml_runtime::ClientSpec::new("openai/gpt-4"),
            baml_runtime::PromptTemplate::new("{{ input }}"),
        );

        // Create context
        let ctx = baml_runtime::context::PerCallContext::new();

        // Build the request
        match baml_runtime::build_request(&prepared, &ctx, stream) {
            Ok(request) => {
                // Convert headers to JSON-friendly format
                let headers: std::collections::HashMap<String, String> = request
                    .headers
                    .iter()
                    .map(|(k, v)| (k.clone(), v.render(false)))
                    .collect();

                BuildRequestResult {
                    success: true,
                    error: None,
                    url: Some(request.url),
                    method: Some(request.method.as_str().to_string()),
                    headers_json: serde_json::to_string(&headers).ok(),
                    body_json: serde_json::to_string(&request.body).ok(),
                }
            }
            Err(e) => BuildRequestResult {
                success: false,
                error: Some(e.to_string()),
                url: None,
                method: None,
                headers_json: None,
                body_json: None,
            },
        }
    }
}

/// Format a function signature as a string.
fn format_signature(sig: &baml_hir::FunctionSignature) -> String {
    let params: Vec<String> = sig
        .params
        .iter()
        .map(|p| format!("{}: {:?}", p.name, p.type_ref))
        .collect();
    format!(
        "{}({}) -> {:?}",
        sig.name,
        params.join(", "),
        sig.return_type
    )
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
