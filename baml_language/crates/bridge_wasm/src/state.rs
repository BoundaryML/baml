//! Stateful WASM bridge: holds the project DB and optional incremental runtime.
//!
//! - **Parse/IDE**: `add_source` / `set_source` + `function_names()` (from DB or runtime).
//! - **Run**: `create_runtime` builds an incremental runtime and syncs the DB into it;
//!   then `call_function` / `function_params` use it. `add_source` after that updates both DB and runtime.

use std::{collections::HashMap, path::Path};

use baml_project::{ProjectDatabase, list_functions};
use bex_factory::{BexIncremental, new_incremental};
use js_sys::Function;
use prost::Message;
use wasm_bindgen::prelude::*;

use crate::wasm_http;

/// Root path for the virtual project (single-file playground).
const ROOT_PATH: &str = "/baml_src";

/// Default main file path under the root.
const MAIN_FILE: &str = "main.baml";

/// Stateful BAML WASM bridge: holds the project DB and optional incremental runtime.
///
/// - Use `add_source` / `set_source` to add or update BAML source.
/// - Use `function_names` to get the list of function names (from DB or runtime).
/// - Use `create_runtime` to build the engine (env + fetch); then `call_function` / `function_params`.
#[wasm_bindgen]
pub struct BamlWasmState {
    db: ProjectDatabase,
    /// Incremental runtime created by `create_runtime`; updated by `add_source` when present.
    runtime: Option<Box<dyn BexIncremental>>,
}

impl Default for BamlWasmState {
    fn default() -> Self {
        Self::new()
    }
}

#[wasm_bindgen]
impl BamlWasmState {
    /// Create a new state with an empty project at the default root.
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        let mut db = ProjectDatabase::new();
        db.set_project_root(Path::new(ROOT_PATH));
        Self { db, runtime: None }
    }

    /// Add or update a source file.
    ///
    /// `path` is the file name or path relative to the project root (e.g. `"main.baml"`).
    /// If a runtime exists, it is updated (recompile); otherwise only the DB is updated.
    #[wasm_bindgen(js_name = addSource)]
    pub fn add_source(&mut self, path: &str, content: &str) {
        let full_path = Path::new(ROOT_PATH).join(path);
        self.db.add_or_update_file(&full_path, content);
        if let Some(rt) = &mut self.runtime {
            let rel = path;
            rt.add_source(rel, content);
        }
    }

    /// Set the main file content (convenience for single-file playground).
    #[wasm_bindgen(js_name = setSource)]
    pub fn set_source(&mut self, content: &str) {
        self.add_source(MAIN_FILE, content);
    }

    /// Return the names of all functions defined in the current project.
    #[wasm_bindgen(js_name = functionNames)]
    pub fn function_names(&self) -> Vec<String> {
        if let Some(rt) = &self.runtime {
            return rt.function_names();
        }
        let Some(project) = self.db.get_project() else {
            return vec![];
        };
        list_functions(&self.db, project)
            .into_iter()
            .map(|s| s.name)
            .collect()
    }

    /// Create the incremental runtime (env + HTTP fetch). Syncs current DB content into it.
    ///
    /// Must be called before `call_function` / `function_params`. If source was updated
    /// after the last `create_runtime`, `add_source`/`set_source` will have updated the runtime already.
    #[wasm_bindgen(js_name = createRuntime)]
    pub fn create_runtime(
        &mut self,
        src_files_json: &str,
        fetch_fn: Function,
    ) -> Result<(), JsError> {
        let src_files: HashMap<String, String> = serde_json::from_str(src_files_json)
            .map_err(|e| JsError::new(&format!("Failed to parse src_files_json: {e}")))?;

        wasm_http::init_http_provider(fetch_fn)
            .map_err(|e| JsError::new(&format!("Failed to init HTTP provider: {e}")))?;

        let sys_ops = sys_types::SysOpsBuilder::new()
            .with_http::<wasm_http::WasmHttp>()
            .build();

        let mut rt = new_incremental(ROOT_PATH, &src_files, sys_ops);
        for (name, text) in Self::db_to_src_files(&self.db) {
            rt.add_source(&name, &text);
        }
        self.runtime = Some(rt);
        Ok(())
    }

    /// Call a BAML function. Requires `create_runtime` to have been called first.
    #[wasm_bindgen(js_name = callFunction)]
    pub async fn call_function(&self, name: &str, args_proto: &[u8]) -> Result<Vec<u8>, JsError> {
        let runtime = self
            .runtime
            .as_ref()
            .ok_or_else(|| JsError::new("Runtime not created: call createRuntime first"))?;

        let args = crate::baml::cffi::HostFunctionArguments::decode(args_proto)
            .map_err(|e| JsError::new(&format!("Failed to decode arguments: {e}")))?;

        let kwargs = crate::kwargs_to_bex_values(args.kwargs)
            .map_err(|e| JsError::new(&format!("Failed to convert arguments: {e}")))?;

        let params = runtime
            .function_params(name)
            .ok_or_else(|| JsError::new(&format!("Function not found: {name}")))?;

        let bex_args: Vec<bex_factory::BexExternalValue> = params
            .iter()
            .map(|(param_name, _param_type)| {
                kwargs.get(*param_name).cloned().ok_or_else(|| {
                    JsError::new(&format!(
                        "Missing argument '{param_name}' for function '{name}'"
                    ))
                })
            })
            .collect::<Result<Vec<_>, _>>()?;

        let result = runtime
            .call_function(name, bex_args)
            .await
            .map_err(|e| JsError::new(&format!("Function call failed: {e}")))?;

        let cffi_value = crate::external_to_cffi_value(&result)
            .map_err(|e| JsError::new(&format!("Failed to encode result: {e}")))?;

        Ok(cffi_value.encode_to_vec())
    }

    /// Get parameter names for a function. Requires `create_runtime` first.
    #[wasm_bindgen(js_name = functionParams)]
    pub fn function_params(&self, name: &str) -> Option<String> {
        let runtime = self.runtime.as_ref()?;
        let params = runtime.function_params(name)?;
        let names: Vec<&str> = params.iter().map(|(n, _)| *n).collect();
        serde_json::to_string(&names).ok()
    }
}

impl BamlWasmState {
    fn db_to_src_files(db: &ProjectDatabase) -> HashMap<String, String> {
        let mut out = HashMap::new();
        for source_file in db.get_source_files() {
            let file_id = source_file.file_id(db);
            let Some(path) = db.get_path(file_id) else {
                continue;
            };
            let name = path
                .file_name()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|| file_id.as_u32().to_string());
            let text = source_file.text(db).clone();
            out.insert(name, text);
        }
        out
    }
}
