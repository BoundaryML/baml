mod asserts;

use std::path::PathBuf;

use pretty_assertions::assert_eq;

use baml_lib::{Configuration, Diagnostics, SourceFile};

#[allow(unused)]
pub(crate) fn parse_unwrap_err(schema: &str) -> String {
    let path = PathBuf::from("./unknown");
    baml_lib::parse_and_validate_schema(&path, vec![SourceFile::from((path.clone(), schema))])
        .map(drop)
        .unwrap_err()
        .to_pretty_string()
}

#[allow(unused)]
pub(crate) fn parse_and_validate_schema(datamodel_string: &str) -> baml_lib::ValidatedSchema {
    let path = PathBuf::from("./unknown");
    baml_lib::parse_and_validate_schema(
        &path,
        vec![SourceFile::from((path.clone(), datamodel_string))],
    )
    .unwrap()
}

#[allow(unused)]
pub(crate) fn parse_config(
    _path: PathBuf,
    schema: &str,
) -> Result<(Configuration, Diagnostics), String> {
    let path = PathBuf::from("./unknown");
    baml_lib::parse_configuration(&path, path.clone(), schema).map_err(|err| err.to_pretty_string())
}

#[allow(unused)]
pub(crate) fn parse_configuration(datamodel_string: &str) -> (Configuration, Diagnostics) {
    let path = PathBuf::from("./unknown");
    match baml_lib::parse_configuration(&path, path.clone(), datamodel_string) {
        Ok(c) => c,
        Err(errs) => {
            panic!(
                "Configuration parsing failed\n\n{}",
                errs.to_pretty_string()
            )
        }
    }
}

#[allow(unused)]
pub(crate) fn expect_error(schema: &str, expectation: &expect_test::Expect) {
    let path = PathBuf::from("./unknown");
    match baml_lib::parse_and_validate_schema(&path, vec![SourceFile::from((path.clone(), schema))])
    {
        Ok(_) => panic!("Expected a validation error, but the schema is valid."),
        Err(err) => assert_eq!(err.errors().first().unwrap().message(), expectation.data()),
    }
}

#[allow(unused)]
pub(crate) fn parse_and_render_error(schema: &str) -> String {
    parse_unwrap_err(schema)
}

#[allow(unused)]
pub(crate) fn assert_valid(schema: &str) {
    let path = PathBuf::from("./unknown");
    match baml_lib::parse_and_validate_schema(&path, vec![SourceFile::from((path.clone(), schema))])
    {
        Ok(_) => (),
        Err(err) => panic!("{}", err.to_pretty_string()),
    }
}
