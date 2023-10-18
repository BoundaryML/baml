mod asserts;

use std::path::PathBuf;

pub(crate) use ::indoc::{formatdoc, indoc};
pub(crate) use asserts::*;
pub(crate) use expect_test::expect;

use baml::{Configuration, SourceFile};

pub(crate) fn parse_unwrap_err(schema: &str) -> String {
    baml::parse_schema(vec![SourceFile::from(("<unknown>".into(), schema))])
        .map(drop)
        .unwrap_err()
        .to_pretty_string()
}

#[track_caller]
pub(crate) fn parse_schema(datamodel_string: &str) -> baml::ValidatedSchema {
    baml::parse_schema(vec![SourceFile::from((
        "<unknown>".into(),
        datamodel_string,
    ))])
    .unwrap()
}

pub(crate) fn parse_config(path: PathBuf, schema: &str) -> Result<Configuration, String> {
    baml::parse_configuration(path, schema).map_err(|err| err.to_pretty_string())
}

#[track_caller]
pub(crate) fn parse_configuration(datamodel_string: &str) -> Configuration {
    match baml::parse_configuration("<unknown>", datamodel_string) {
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
    match baml::parse_schema(vec![SourceFile::from(("<unknown>".into(), schema))]) {
        Ok(_) => panic!("Expected a validation error, but the schema is valid."),
        Err(err) => expectation.assert_eq(&err.to_pretty_string()),
    }
}

pub(crate) fn parse_and_render_error(schema: &str) -> String {
    parse_unwrap_err(schema)
}

#[track_caller]
pub(crate) fn assert_valid(schema: &str) {
    match baml::parse_schema(vec![SourceFile::from(("<unknown>".into(), schema))]) {
        Ok(_) => (),
        Err(err) => panic!("{}", err.to_pretty_string()),
    }
}
