use crate::common::*;

#[test]
fn default_classes_test() {
    let schema = r#"
      /// Some doc string
      function FooBar {
        input string
        output Bar
      }
    "#;

    assert_valid(schema);
}
