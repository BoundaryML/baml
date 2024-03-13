#[cfg(target_arch = "wasm32")]
use log::info;
#[cfg(target_arch = "wasm32")]
use std::path::PathBuf;

use crate::ast::*;
use internal_baml_diagnostics::{DatamodelError, Diagnostics, SourceFile};
use serde::Deserialize;
use serde_json::Value;

// Define an enum for the different types of input
#[derive(Deserialize, Debug)]
#[serde(untagged)] // This allows for different shapes of JSON
enum Input {
    ObjectInput(Value), // Use serde_json::Value for a generic JSON object
}

impl Input {
    // Method to get the string representation of the input
    fn to_string(&self) -> String {
        match self {
            Input::ObjectInput(obj) => serde_json::to_string(obj).unwrap_or_default(),
        }
    }
}

#[derive(Deserialize, Debug)]
struct TestFileContent {
    input: Input,
}

pub(crate) fn parse_test_from_json(
    source: &SourceFile,
    diagnostics: &mut Diagnostics,
) -> Result<SchemaAst, Diagnostics> {
    // Path relative to the root of the project.
    let source_path = source.path_buf().clone();
    let root_path = diagnostics.root_path.clone();

    #[cfg(not(target_arch = "wasm32"))]
    let relative_path = source_path.strip_prefix(&root_path);

    #[cfg(target_arch = "wasm32")]
    let relative_path = match source_path
        .to_string_lossy()
        .to_string()
        .strip_prefix(&root_path.to_string_lossy().to_string())
    {
        Some(path) => {
            // Remove the leading slash or backslash.
            let path = match path.chars().next() {
                Some('/') => &path[1..],
                Some('\\') => &path[1..],
                _ => path,
            };
            Ok(PathBuf::from(path))
        }
        None => Err(()),
    };

    match relative_path {
        Err(_) => {
            diagnostics.push_error(DatamodelError::new_validation_error(
                &format!(
                    "The path of the test file must be inside the project root: {} {}",
                    root_path.display(),
                    source_path.display()
                ),
                Span::empty(source.clone()),
            ));
        }
        _ => (),
    };

    diagnostics.to_result()?;
    let relative_path = relative_path.unwrap();

    #[cfg(target_arch = "wasm32")]
    let relative_path = match relative_path.to_string_lossy().contains("\\") {
        true => {
            // replace all \\ with /
            PathBuf::from(
                relative_path
                    .to_string_lossy()
                    .to_string()
                    .replace("\\", "/"),
            )
        }
        false => relative_path,
    };

    let parts = relative_path.components();

    // Ensure is of the form `__tests__/<function_name>/(<group_name>/)/<test_name>.json` using regex
    // or throw an error.
    let mut function_name = None;
    let mut test_name = None;
    let mut group_name = None;
    for (idx, part) in parts.enumerate() {
        let part = part.as_os_str().to_str().unwrap();
        match idx {
            0 => {
                if part != "__tests__" {
                    diagnostics.push_error(DatamodelError::new_validation_error(
                        "A BAML test file must be in a `__tests__` directory.",
                        Span::empty(source.clone()),
                    ));
                }
            }
            1 => {
                function_name = Some(part);
            }
            _ => {
                if part.ends_with(".json") {
                    test_name = Some(
                        part.strip_suffix(".json")
                            .unwrap()
                            .replace(|c: char| !c.is_alphanumeric() && c != '_', "_"),
                    );
                } else {
                    group_name = match group_name {
                        None => Some(part.to_string()),
                        Some(prev) => Some(format!("{}_{}", prev, part)),
                    }
                }
            }
        }
    }

    if function_name.is_none() {
        diagnostics.push_error(DatamodelError::new_validation_error(
            "Missing a function name in the path.",
            Span::empty(source.clone()),
        ));
    }

    if test_name.is_none() {
        diagnostics.push_error(DatamodelError::new_validation_error(
            "Test file must have a name",
            Span::empty(source.clone()),
        ));
    }

    diagnostics.to_result()?;

    let function_name = function_name.unwrap();
    let test_name = test_name.unwrap();

    let file_content: TestFileContent = match serde_json::from_str(source.as_str()) {
        Ok(file_content) => file_content,
        Err(err) => {
            diagnostics.push_error(DatamodelError::new_validation_error(
                &format!("Failed to parse JSON: {}", err),
                Span::empty(source.clone()),
            ));
            diagnostics.to_result()?;
            unreachable!()
        }
    };
    let test_input = file_content.input.to_string();
    let end_range = test_input.len();
    let span = Span::new(source.clone(), 0, end_range);
    let content = match file_content.input {
        Input::ObjectInput(val) => {
            Expression::from_json(val, span.clone(), Span::empty(source.clone()))
        }
    };
    let test_case = ConfigBlockProperty {
        name: Identifier::Local("input".into(), span.clone()),
        value: Some(content),
        template_args: None,
        attributes: vec![],
        documentation: None,
        span: span.clone(),
    };
    let function_name = ConfigBlockProperty {
        name: Identifier::Local("function".into(), span.clone()),
        value: Some(Expression::StringValue(function_name.into(), span.clone())),
        template_args: None,
        attributes: vec![],
        documentation: None,
        span: span.clone(),
    };
    let mut top = RetryPolicyConfig {
        name: Identifier::Local(test_name.into(), span.clone()),
        documentation: None,
        attributes: vec![],
        fields: vec![test_case, function_name],
        span: span.clone(),
    };
    if let Some(group_name) = group_name {
        top.fields.push(ConfigBlockProperty {
            name: Identifier::Local("group".into(), span.clone()),
            value: Some(Expression::StringValue(group_name.into(), span.clone())),
            template_args: None,
            attributes: vec![],
            documentation: None,
            span: span.clone(),
        });
    }
    Ok(SchemaAst {
        tops: vec![Top::Config(Configuration::TestCase(top))],
    })
}
