use crate::objects::Enum;

baml_codegen_types::render_fn! {
    /// ```askama
    /// class {{enum_.name.render(crate::ty::Namespace::Types)}}(str, Enum):
    ///     {%- if let Some(docstring) = enum_.docstring %}
    ///     {{ docstring.as_docstring()|indent(4) }}
    ///     {% endif -%}
    ///
    ///     {% for variant in enum_.variants %}
    ///     {% if let Some(docstring) = variant.docstring -%}
    ///     {{ docstring.as_comment() }}
    ///     {% endif -%}
    ///     {{ variant.name }} = {{ variant.value }}
    ///     {%- endfor %}
    ///     {%- if enum_.variants.is_empty() && enum_.docstring.is_none() %}
    ///     pass
    ///     {% endif %}
    /// ```
    pub fn print(enum_: &Enum) -> String;
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    macro_rules! test_enum_render {
        (
            $test_name:ident:
            enum $name:ident $(@ $doc:literal)? {
                $($variant:ident = $value:literal $(@ $vdoc:literal)?),* $(,)?
            }
            =>
            $expected:expr
        ) => {
            #[test]
            fn $test_name() {
                let enum_ = Enum::from_codegen_types(&baml_codegen_tests::r#enum!(
                    $name $(@ $doc)? {
                        $($variant = $value $(@ $vdoc)?),*
                    }
                ));
                assert_eq!(
                    print(&enum_),
                    crate::docstring::dedent($expected).trim()
                );
            }
        };
    }

    test_enum_render! {
        enum_with_docs:
        enum MyEnum @ "My docstring" {
            Variant = "Variant as string" @ "variant docs",
        }
        =>
        r#"
            class MyEnum(str, Enum):
                """My docstring"""
                
                # variant docs
                Variant = "Variant as string"
            "#
    }

    test_enum_render! {
        enum_no_docs:
        enum Status {
            Active = "active",
            Inactive = "inactive",
        }
        =>
        r#"
            class Status(str, Enum):
                Active = "active"
                Inactive = "inactive"
            "#
    }

    test_enum_render! {
        enum_only_enum_doc:
        enum Priority @ "Task priority levels" {
            Low = "low",
            Medium = "medium",
            High = "high",
        }
        =>
        r#"
            class Priority(str, Enum):
                """Task priority levels"""
                
                Low = "low"
                Medium = "medium"
                High = "high"
            "#
    }

    test_enum_render! {
        enum_only_variant_docs:
        enum Color {
            Red = "red" @ "The color red",
            Green = "green" @ "The color green",
            Blue = "blue",
        }
        =>
        r#"
            class Color(str, Enum):
                # The color red
                Red = "red"
                # The color green
                Green = "green"
                Blue = "blue"
            "#
    }

    test_enum_render! {
        enum_multiline_docstring:
        enum Animal @ "Different types of animals.\nUsed for classification." {
            Dog = "dog" @ "A loyal companion",
            Cat = "cat",
        }
        =>
        r#"
            class Animal(str, Enum):
                """
                Different types of animals.
                Used for classification.
                """
                
                # A loyal companion
                Dog = "dog"
                Cat = "cat"
            "#
    }

    test_enum_render! {
        enum_no_variants:
        enum Empty {
        }
        =>
        r#"
            class Empty(str, Enum):
                pass
            "#
    }

    test_enum_render! {
        enum_no_variants_with_doc:
        enum Empty @ "Empty enum" {
        }
        =>
        r#"
            class Empty(str, Enum):
                """Empty enum"""
            "#
    }
}
