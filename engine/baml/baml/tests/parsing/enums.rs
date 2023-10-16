use crate::common::*;

#[test]
fn default_enum_test() {
    let schema = r#"
      /// Add a docstring for the enum
      /// By doing this
      enum FooBar {
        FOO // You can add comments here

        // Or here!
        BAR @attribute(a)
        @attribute(b)

        @@rename(b)
      }
    "#;

    assert_valid(schema);
}
