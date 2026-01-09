mod hot_reload_testdata;

use std::path::{Path, PathBuf};

use baml_project::{ProjectDatabase, list_functions};
pub use hot_reload_testdata::hot_reload_test_string;
use wasm_bindgen::prelude::*;

#[cfg(feature = "console_error_panic")]
extern crate console_error_panic_hook;

#[cfg(feature = "small_allocator")]
#[global_allocator]
static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;

#[wasm_bindgen(start)]
pub fn start() {
    #[cfg(feature = "console_error_panic")]
    console_error_panic_hook::set_once();
}

/// Returns the version of the BAML compiler.
#[wasm_bindgen]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

/// A BAML project wrapper for WASM.
#[wasm_bindgen]
pub struct BamlProject {
    db: ProjectDatabase,
    file_path: PathBuf,
}

#[wasm_bindgen]
impl BamlProject {
    #[wasm_bindgen(constructor)]
    pub fn new(baml_src: String) -> BamlProject {
        let mut db = ProjectDatabase::new();
        let file_path = PathBuf::from("/baml_src/main.baml");

        // Create a project with a virtual file path
        db.set_project_root(Path::new("/baml_src"));

        // Add the source file to the database
        db.add_file(&file_path, &baml_src);

        BamlProject { db, file_path }
    }

    /// Allows updating the stored BAML source.
    ///
    /// This uses Salsa's incremental computation - only queries affected
    /// by the text change will be recomputed on subsequent calls.
    #[wasm_bindgen]
    pub fn set_source(&mut self, baml_src: String) {
        self.db.add_or_update_file(&self.file_path, &baml_src);
    }

    /// Returns the names of all functions defined in the BAML project.
    ///
    /// This uses Salsa's tracked queries, which:
    /// 1. Depend on `project_items` → `file_items` → `file_item_tree`
    /// 2. Are memoized - subsequent calls return cached results if source unchanged
    /// 3. Only recompute when function signatures change (not body edits)
    #[wasm_bindgen]
    pub fn function_names(&self) -> Vec<String> {
        if let Some(project) = self.db.project() {
            let symbols = list_functions(&self.db, project);
            symbols.into_iter().map(|s| s.name).collect()
        } else {
            vec![]
        }
    }
}
