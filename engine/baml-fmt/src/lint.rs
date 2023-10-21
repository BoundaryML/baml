use serde::Deserialize;
use std::{path::PathBuf, sync::Arc};

use baml::{
    internal_baml_diagnostics::{DatamodelError, DatamodelWarning},
    SourceFile,
};

#[derive(serde::Serialize)]
pub struct MiniError {
    start: usize,
    end: usize,
    text: String,
    is_warning: bool,
    source_file: String,
}

#[derive(Deserialize)]
struct File {
    path: String,
    content: String,
}

#[derive(Deserialize)]
struct Input {
    root_path: String,
    files: Vec<File>,
}

pub(crate) fn run(input: &str) -> String {
    let input: Input = serde_json::from_str(input).expect("Failed to parse input");

    let files: Vec<SourceFile> = input
        .files
        .into_iter()
        .map(|file| SourceFile::new_allocated(file.path.into(), Arc::from(file.content)))
        .collect();

    let path = PathBuf::from(input.root_path);
    let schema = baml::validate(&path, files);
    let diagnostics = &schema.diagnostics;

    let mut mini_errors: Vec<MiniError> = diagnostics
        .errors()
        .iter()
        .map(|err: &DatamodelError| MiniError {
            start: err.span().start,
            end: err.span().end,
            text: err.message().to_string(),
            is_warning: false,
            source_file: err.span().file.path(),
        })
        .collect();

    let mut mini_warnings: Vec<MiniError> = diagnostics
        .warnings()
        .iter()
        .map(|warn: &DatamodelWarning| MiniError {
            start: warn.span().start,
            end: warn.span().end,
            text: warn.message().to_owned(),
            is_warning: true,
            source_file: warn.span().file.path(),
        })
        .collect();

    mini_errors.append(&mut mini_warnings);

    print_diagnostics(mini_errors)
}

fn print_diagnostics(diagnostics: Vec<MiniError>) -> String {
    serde_json::to_string(&diagnostics).expect("Failed to render JSON")
}

#[cfg(test)]
mod tests {
    use expect_test::expect;
    use indoc::indoc;

    fn lint(s: &str) -> String {
        let result = super::run(s);
        let value: serde_json::Value = serde_json::from_str(&result).unwrap();

        serde_json::to_string_pretty(&value).unwrap()
    }

    #[test]
    fn deprecated_preview_features_should_give_a_warning() {
        let dml = indoc! {r#"
            datasource db {
              provider = "postgresql"
              url      = env("DATABASE_URL")
            }

            generator client {
              provider = "prisma-client-js"
              previewFeatures = ["createMany"]
            }

            model A {
              id  String   @id
            }
        "#};

        let expected = expect![[r#"
            [
              {
                "start": 149,
                "end": 163,
                "text": "Preview feature \"createMany\" is deprecated. The functionality can be used without specifying it as a preview feature.",
                "is_warning": true
              }
            ]"#]];

        expected.assert_eq(&lint(dml));
    }
}
