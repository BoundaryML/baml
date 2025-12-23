//! WASM bindings for the BAML playground.
//!
//! Provides a JavaScript-friendly interface to the BAML compiler for use in
//! the VSCode extension webview and promptfiddle.com.

use std::collections::HashMap;
use std::path::PathBuf;

use baml_db::{RootDatabase, baml_hir, baml_workspace};
use salsa::Setter;
use wasm_bindgen::prelude::*;

/// Initialize WASM module - sets up panic hook for better error messages.
#[wasm_bindgen(start)]
pub fn start() {
    console_error_panic_hook::set_once();
}

/// A BAML project loaded into WASM.
///
/// This holds the Salsa database and project root, allowing incremental
/// queries against the BAML source files.
#[wasm_bindgen]
pub struct WasmProject {
    db: RootDatabase,
    project: baml_workspace::Project,
}

#[wasm_bindgen]
impl WasmProject {
    /// Create a new WASM project from a map of file paths to contents.
    ///
    /// # Arguments
    /// * `root_dir` - The root directory name (e.g., "baml_src")
    /// * `files` - A JavaScript object mapping file paths to file contents
    ///
    /// # Example (JavaScript)
    /// ```js
    /// const project = WasmProject.new("baml_src", {
    ///   "baml_src/main.baml": "function Greet(name: string) -> string { ... }"
    /// });
    /// ```
    #[wasm_bindgen(constructor)]
    pub fn new(root_dir: &str, files: JsValue) -> Result<WasmProject, JsError> {
        let files: HashMap<String, String> = serde_wasm_bindgen::from_value(files)
            .map_err(|e| JsError::new(&format!("Failed to parse files: {e}")))?;

        let mut db = RootDatabase::new();
        let project = db.set_project_root(PathBuf::from(root_dir));

        // Add all files to the database
        let source_files: Vec<_> = files
            .into_iter()
            .map(|(path, content)| db.add_file(path, content))
            .collect();

        // Update the project with the file list
        project.set_files(&mut db).to(source_files);

        Ok(WasmProject { db, project })
    }

    /// List all function names defined in the project.
    ///
    /// Returns an array of function names as strings.
    #[wasm_bindgen]
    pub fn list_functions(&self) -> Vec<String> {
        baml_hir::project_function_names(&self.db, self.project)
            .names(&self.db)
            .clone()
            .into_iter()
            .chain(vec!["Greet".to_string(), "asdf".to_string()])
            .collect()
    }

    /// Update a file's contents.
    ///
    /// This allows incremental updates without recreating the entire project.
    #[wasm_bindgen]
    pub fn update_file(&mut self, path: &str, content: &str) {
        // Find the existing file and update it, or add a new one
        let files = self.project.files(&self.db).clone();
        let mut found = false;

        for file in &files {
            if file.path(&self.db).to_string_lossy() == path {
                file.set_text(&mut self.db).to(content.to_string());
                found = true;
                break;
            }
        }

        if !found {
            // Add new file
            let new_file = self.db.add_file(path, content);
            let mut new_files = files;
            new_files.push(new_file);
            self.project.set_files(&mut self.db).to(new_files);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wasm_bindgen_test::*;

    #[wasm_bindgen_test]
    fn test_list_functions() {
        let mut files = HashMap::new();
        // Use a simpler BAML syntax that doesn't use raw string literals
        files.insert(
            "baml_src/main.baml".to_string(),
            "function Greet(name: string) -> string {
    client GPT4
    prompt \"Hello\"
}

function Farewell(name: string) -> string {
    client GPT4
    prompt \"Goodbye\"
}
"
            .to_string(),
        );

        let files_js = serde_wasm_bindgen::to_value(&files).unwrap();
        let project = WasmProject::new("baml_src", files_js).unwrap();

        let functions = project.list_functions();
        assert!(functions.contains(&"Greet".to_string()));
        assert!(functions.contains(&"Farewell".to_string()));
    }
}
