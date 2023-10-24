mod asserts;

use std::path::PathBuf;

pub(crate) use ::indoc::{formatdoc, indoc};
pub(crate) use asserts::*;
pub(crate) use expect_test::expect;
use pretty_assertions::{assert_eq, assert_ne};

use baml::{Configuration, SourceFile};

pub(crate) fn parse_unwrap_err(schema: &str) -> String {
    let path = PathBuf::from("./unknown");
    baml::parse_and_validate_schema(&path, vec![SourceFile::from((path.clone(), schema))])
        .map(drop)
        .unwrap_err()
        .to_pretty_string()
}

#[track_caller]
pub(crate) fn parse_and_validate_schema(datamodel_string: &str) -> baml::ValidatedSchema {
    let path = PathBuf::from("./unknown");
    baml::parse_and_validate_schema(
        &path,
        vec![SourceFile::from((path.clone(), datamodel_string))],
    )
    .unwrap()
}

pub(crate) fn parse_config(path: PathBuf, schema: &str) -> Result<Configuration, String> {
    let path = PathBuf::from("./unknown");
    baml::parse_configuration(&path, path.clone(), schema).map_err(|err| err.to_pretty_string())
}

#[track_caller]
pub(crate) fn parse_configuration(datamodel_string: &str) -> Configuration {
    let path = PathBuf::from("./unknown");
    match baml::parse_configuration(&path, path.clone(), datamodel_string) {
        Ok(c) => c,
        Err(errs) => {
            panic!(
                "Configuration parsing failed\n\n{}",
                errs.to_pretty_string()
            )
        }
    }
}

#[track_caller]
pub(crate) fn expect_error(schema: &str, expectation: &expect_test::Expect) {
    let path = PathBuf::from("./unknown");
    match baml::parse_and_validate_schema(&path, vec![SourceFile::from((path.clone(), schema))]) {
        Ok(_) => panic!("Expected a validation error, but the schema is valid."),
        Err(err) => assert_eq!(err.errors().get(0).unwrap().message(), expectation.data()),
    }
}

pub(crate) fn parse_and_render_error(schema: &str) -> String {
    parse_unwrap_err(schema)
}

#[track_caller]
pub(crate) fn assert_valid(schema: &str) {
    let path = PathBuf::from("./unknown");
    match baml::parse_and_validate_schema(&path, vec![SourceFile::from((path.clone(), schema))]) {
        Ok(_) => (),
        Err(err) => panic!("{}", err.to_pretty_string()),
    }
}
