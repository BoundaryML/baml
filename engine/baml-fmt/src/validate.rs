use serde::Deserialize;
use serde_json::json;
use std::{fmt::Write as _, path::PathBuf};

// this mirrors user_facing_errors::common::SchemaParserError
pub(crate) static SCHEMA_PARSER_ERROR_CODE: &str = "P1012";

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct ValidateParams {
    prisma_schema: String,
    #[serde(default)]
    no_color: bool,
}

pub(crate) fn validate(params: &str) -> Result<(), String> {
    let params: ValidateParams = match serde_json::from_str(params) {
        Ok(params) => params,
        Err(serde_err) => {
            panic!("Failed to deserialize ValidateParams: {serde_err}");
        }
    };

    run(&params.prisma_schema, params.no_color)
}

pub fn run(input_schema: &str, no_color: bool) -> Result<(), String> {
    let single_elem_vec = vec![baml_lib::SourceFile::from((
        "<unknown>".into(),
        input_schema,
    ))];
    let path = PathBuf::from("./unknown");
    let validate_schema = baml_lib::validate(&path, single_elem_vec);
    let diagnostics = &validate_schema.diagnostics;

    if !diagnostics.has_errors() {
        return Ok(());
    }

    // always colorise output regardless of the environment, which is important for Wasm
    colored::control::set_override(!no_color);

    let mut formatted_error = diagnostics.to_pretty_string();
    write!(
        formatted_error,
        "\nValidation Error Count: {}",
        diagnostics.errors().len(),
    )
    .unwrap();
    Err(json!({
        "error_code": SCHEMA_PARSER_ERROR_CODE,
        "message": formatted_error,
    })
    .to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_paths() {
        let path = PathBuf::from("__tests__\\Thing\\entire_apricot.json");
        let components = path.components();
        path.components()
            .map(|c| c.as_os_str().to_str().unwrap())
            .for_each(|c| println!("{}", c));
        print!("{:?}", components);
    }
}
