use crate::common::parse_schema;
#[cfg(test)]
use pretty_assertions::{assert_eq, assert_ne};

#[test]
fn test_validate() {
    const datamodel_string: &str = r#"
class Hello {
    world String
}
"#;
    let schema = parse_schema(datamodel_string);
    let diagnostics = schema.diagnostics;
    let warnings = diagnostics.into_warnings();
    assert_eq!(warnings.len(), 1);
    let first_diagnostic = warnings.get(0).unwrap();
    assert_eq!(first_diagnostic.message(), "random message");
}
