use std::{fs, process::Command};

use assert_cmd::prelude::*;

#[test]
fn json_schema_import_generates_baml_that_passes_check() {
    let project = tempfile::tempdir().expect("temporary project should be created");
    let schema_path = project.path().join("resume.schema.json");
    fs::write(
        &schema_path,
        r##"{
          "title": "Resume",
          "type": "object",
          "required": ["name", "entries"],
          "properties": {
            "name": { "type": "string" },
            "entries": {
              "type": "array",
              "items": { "$ref": "#/$defs/Entry" }
            },
            "preferred-contact": {
              "anyOf": [{ "type": "string" }, { "type": "null" }]
            }
          },
          "$defs": {
            "Entry": {
              "type": "object",
              "required": ["kind"],
              "properties": {
                "kind": { "$ref": "#/$defs/EntryKind" },
                "next": { "$ref": "#/$defs/Entry" }
              }
            },
            "EntryKind": {
              "type": "string",
              "enum": ["education", "work-experience"]
            }
          }
        }"##,
    )
    .expect("schema fixture should be written");

    Command::cargo_bin("baml-cli")
        .expect("baml-cli binary should build")
        .current_dir(project.path())
        .args([
            "import",
            "--from",
            "jsonschema",
            schema_path.to_str().expect("path should be UTF-8"),
        ])
        .assert()
        .success();

    let generated = project.path().join("baml_src/generated.baml");
    assert!(generated.exists());
    Command::cargo_bin("baml-cli")
        .expect("baml-cli binary should build")
        .current_dir(project.path())
        .args(["check", "--from", "baml_src"])
        .assert()
        .success();
}

#[test]
fn pydantic_import_extracts_models_before_converting() {
    let project = tempfile::tempdir().expect("temporary project should be created");
    let package = project.path().join("pydantic");
    fs::create_dir(&package).expect("fake pydantic package should be created");
    fs::write(package.join("__init__.py"), "class BaseModel:\n    pass\n")
        .expect("fake pydantic package should be written");
    let models = project.path().join("models.py");
    fs::write(
        &models,
        r#"from pydantic import BaseModel

class Person(BaseModel):
    @classmethod
    def schema(cls, ref_template=None):
        return {
            "title": "Person",
            "type": "object",
            "required": ["name"],
            "properties": {
                "name": {"type": "string"},
                "tags": {"type": "array", "items": {"type": "string"}}
            }
        }
"#,
    )
    .expect("Pydantic fixture should be written");

    Command::cargo_bin("baml-cli")
        .expect("baml-cli binary should build")
        .current_dir(project.path())
        .env("PYTHONPATH", project.path())
        .args([
            "import",
            "--from",
            "pydantic",
            models.to_str().expect("path should be UTF-8"),
            "--out",
            "baml_src/models.baml",
        ])
        .assert()
        .success();

    let generated = fs::read_to_string(project.path().join("baml_src/models.baml"))
        .expect("generated BAML should be readable");
    assert!(generated.contains("class Person"));
    assert!(generated.contains("tags string[]?"));
    Command::cargo_bin("baml-cli")
        .expect("baml-cli binary should build")
        .current_dir(project.path())
        .args(["check", "--from", "baml_src"])
        .assert()
        .success();
}
