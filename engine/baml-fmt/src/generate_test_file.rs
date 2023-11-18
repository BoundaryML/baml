use std::{path::PathBuf, sync::Arc};

use baml_lib::{internal_baml_core::TestRequest, SourceFile};
use serde::Deserialize;
use serde_json::json;

#[derive(Deserialize)]
struct File {
    path: String,
    content: String,
}

#[derive(Deserialize)]
struct Input {
    root_path: String,
    files: Vec<File>,
    test_request: TestRequest,
}

pub(crate) fn run(input: &str) -> String {
    match serde_json::from_str::<Input>(input) {
        Ok(input) => {
            let files = input
                .files
                .into_iter()
                .map(|file| SourceFile::new_allocated(file.path.into(), Arc::from(file.content)))
                .collect();

            let path = PathBuf::from(input.root_path);
            let schema = baml_lib::validate(&path, files);
            let diagnostics = &schema.diagnostics;

            if diagnostics.has_errors() {
                return json!({
                    "status": "error",
                    "message": "Validation failed",
                })
                .to_string();
            }
            match input.test_request.generate_python(&schema.db) {
                Ok(res) => json!({
                    "status": "ok",
                    "content": res,
                })
                .to_string(),
                Err(e) => json!({
                    "status": "error",
                    "message": e.join("\n"),
                })
                .to_string(),
            }
        }
        Err(e) => json!({
            "status": "error",
            "message": format!("Failed to parse input: {} {}", input, e),
        })
        .to_string(),
    }
}
