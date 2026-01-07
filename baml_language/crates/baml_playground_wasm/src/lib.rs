use std::path::PathBuf;

use baml_db::{RootDatabase, SourceFile, baml_hir, baml_workspace};
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
        foo.push("injected-hot-reload5".to_string());
        foo
    }

    /// Convenience helper returning the raw BAML source currently stored.
    #[wasm_bindgen(getter)]
    pub fn baml_src(&self) -> String {
        self.baml_src.clone()
    }
}
