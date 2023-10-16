mod asserts;

pub(crate) use ::indoc::{formatdoc, indoc};
pub(crate) use asserts::*;
pub(crate) use expect_test::expect;

use baml::Configuration;

pub(crate) fn parse_unwrap_err(schema: &str) -> String {
    baml::parse_schema(schema).map(drop).unwrap_err()
}

#[track_caller]
pub(crate) fn parse_schema(datamodel_string: &str) -> baml::ValidatedSchema {
    baml::parse_schema(datamodel_string).unwrap()
}

pub(crate) fn parse_config(schema: &str) -> Result<Configuration, String> {
    baml::parse_configuration(schema).map_err(|err| err.to_pretty_string("schema.prisma", schema))
}

#[track_caller]
pub(crate) fn parse_configuration(datamodel_string: &str) -> Configuration {
    match baml::parse_configuration(datamodel_string) {
        Ok(c) => c,
        Err(errs) => {
            panic!(
                "Configuration parsing failed\n\n{}",
                errs.to_pretty_string("", datamodel_string)
            )
        }
    }
}

#[track_caller]
pub(crate) fn expect_error(schema: &str, expectation: &expect_test::Expect) {
    match baml::parse_schema(schema) {
        Ok(_) => panic!("Expected a validation error, but the schema is valid."),
        Err(err) => expectation.assert_eq(&err),
    }
}

pub(crate) fn parse_and_render_error(schema: &str) -> String {
    parse_unwrap_err(schema)
}

#[track_caller]
pub(crate) fn assert_valid(schema: &str) {
    match baml::parse_schema(schema) {
        Ok(_) => (),
        Err(err) => panic!("{err}"),
    }
}
