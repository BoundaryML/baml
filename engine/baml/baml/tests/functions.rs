// use pretty_assertions::{assert_eq, assert_ne};

// use crate::common::{expect_error, parse_and_validate_schema};

// #[test]
// fn test_validate() {
//     const datamodel_string: &str = r#"
// class Two {
//   hi String
// }

// function Foo {
//   input One
//   output Two
// }
// "#;
//     // TODO: expect the message to be equal to the one in the error
//     let expectation = expect_test::expect![[r#"Error validatin: Hi there"#]];
//     let schema = expect_error(datamodel_string, &expectation);

//     // let diagnostics = schema.diagnostics;
//     // println!("{:?}", diagnostics.to_pretty_string());
//     // let first_diagnostic = warnings.get(0).unwrap();
//     // assert_eq!(first_diagnostic.message(), "random message");
// }
