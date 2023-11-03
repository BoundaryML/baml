use crate::common::*;

#[test]
fn trailing_comments_allowed_in_configuration_blocks() {
    let schema = r#"
      // This is a random commet

      // Anther random comment
      // But in a block

    "#;

    assert_valid(schema);
}
