use crate::common::*;

#[test]
fn default_classes_test() {
    let schema = r#"
      class User {
        id int
        name string
      }
    "#;

    assert_valid(schema);
}

#[test]
fn default_classes_with_attributes_test2() {
    let schema = r#"
    class User {
      id int
      name string
      @rename({
        key {
          name "id"
        }
      })
      @id
      @foobar(hi)
    }
  "#;

  assert_valid(schema);
}

// #[test]
// fn default_classes_with_attributes_test() {
//   let schema = r#"
//     class User {
//       id int @id
//       name string
//     }
//   "#;

//   let expected_error = expect![[r#"
//   [1;91merror[0m: [1m@attributes not yet supported[0m
//     [1;94m-->[0m  [4mschema.prisma:3[0m
//   [1;94m   | [0m
//   [1;94m 2 | [0m    class User {
//   [1;94m 3 | [0m      id int [1;91m@id[0m
//   [1;94m   | [0m
//   "#]];

//   expect_error(
//     schema,
//     &expected_error,
//   );
// }
