use super::*;
use pretty_assertions::assert_eq;
use unindent::Unindent as _;

#[track_caller]
fn assert_format_eq(schema: &str, expected: &str) -> Result<()> {
    let formatted = format_schema(
        &schema,
        FormatOptions {
            indent_width: 2,
            fail_on_unhandled_rule: true,
        },
    )?;
    assert_eq!(formatted, expected);

    Ok(())
}

#[test]
fn class_containing_whitespace() -> anyhow::Result<()> {
    let actual = r#"
          class Foo {
          }

          class Foo { field1 string }

          class Foo {

            field1 string
          }

          class Foo {
              field1   string|int
          }
        "#
    .unindent()
    .trim_end()
    .to_string();

    let expected = r#"
          class Foo {}

          class Foo {
            field1 string
          }

          class Foo {
            field1 string
          }

          class Foo {
            field1 string | int
          }
        "#
    .unindent()
    .trim_end()
    .to_string();

    assert_format_eq(&actual, &expected)?;
    assert_format_eq(&expected, &expected)
}

#[test]
fn class_with_assorted_comment_styles() -> anyhow::Result<()> {
    let actual = r#"
    class Foo0 {
      lorem string    // trailing comments should be separated by two spaces
      ipsum string
    }

    class Foo1 {
       lorem string
      ipsum string
        // dolor string
    }

    class Foo2 {

        // "lorem" is a latin word
        lorem string

        // "ipsum" is a latin word
        ipsum string

    }

    class Foo3 {
      lorem string
      ipsum string
                    // Lorem ipsum dolor sit amet
      // Consectetur adipiscing elit
            // Sed do eiusmod tempor incididunt
      // Ut labore et dolore magna aliqua
        // Ut enim ad minim veniam
    }
        "#
    .unindent()
    .trim_end()
    .to_string();

    let expected = r#"
    class Foo0 {
      lorem string  // trailing comments should be separated by two spaces
      ipsum string
    }

    class Foo1 {
      lorem string
      ipsum string
      // dolor string
    }

    class Foo2 {
      // "lorem" is a latin word
      lorem string
      // "ipsum" is a latin word
      ipsum string
    }

    class Foo3 {
      lorem string
      ipsum string
      // Lorem ipsum dolor sit amet
      // Consectetur adipiscing elit
      // Sed do eiusmod tempor incididunt
      // Ut labore et dolore magna aliqua
      // Ut enim ad minim veniam
    }
        "#
    .unindent()
    .trim_end()
    .to_string();

    assert_format_eq(&actual, &expected)?;
    assert_format_eq(&expected, &expected)
}

#[test]
fn baml_format_escape_directive_works() -> anyhow::Result<()> {
    let expected = r#"
    // baml-format: ignore
    class BadlyFormatted0 {
        lorem string    // trailing comments should be separated by two spaces
  ipsum string
    }

    class BadlyFormatted1 {
      lorem string
      ipsum string
                    // Lorem ipsum dolor sit amet
      // Consectetur adipiscing elit
            // Sed do eiusmod tempor incididunt
      // Ut labore et dolore magna aliqua
        // Ut enim ad minim veniam
    }
        "#
    .unindent()
    .trim_end()
    .to_string();

    assert_format_eq(&expected, &expected)
}

/// We have not yet implemented formatting for functions or enums,
/// so those should be preserved as-is.
#[test]
fn class_formatting_is_resilient_to_unhandled_rules() -> anyhow::Result<()> {
    let actual = r##"
    function      LlmConvert(input: string) -> string {
    client    "openai/gpt-4o"
            prompt #"
              Extract this info from the email in JSON format:
              {{ ctx.output_format }}
            "#
    }

    enum Latin {
                    Lorem
    Ipsum
    }

    class Foo {
          lorem     "alpha" | "bravo"
    ipsum "charlie"|"delta"
    }
    "##
    .unindent()
    .trim_end()
    .to_string();
    let expected = r##"
    function      LlmConvert(input: string) -> string {
    client    "openai/gpt-4o"
            prompt #"
              Extract this info from the email in JSON format:
              {{ ctx.output_format }}
            "#
    }

    enum Latin {
                    Lorem
    Ipsum
    }

    class Foo {
      lorem "alpha" | "bravo"
      ipsum "charlie" | "delta"
    }
        "##
    .unindent()
    .trim_end()
    .to_string();

    assert_format_eq(&actual, &expected)
}

#[test]
fn newlines_with_only_spaces_are_stripped() -> anyhow::Result<()> {
    let actual = "class Foo {}\n     \n     \nclass Bar {}\n";
    let expected = "class Foo {}\n\n\nclass Bar {}\n";

    assert_format_eq(&actual, &expected)
}
