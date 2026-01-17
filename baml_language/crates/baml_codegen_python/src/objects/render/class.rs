use crate::{objects::Class, ty::Namespace};

baml_codegen_types::render_fn! {
    /// ```askama
    /// class {{class_.name.render(*namespace)}}(BaseModel):
    ///     {%- if let Some(docstring) = class_.docstring %}
    ///     {{ docstring.as_docstring()|indent(4) }}
    ///     {% endif -%}
    ///
    ///     {% for property in class_.properties %}
    ///     {% if let Some(docstring) = property.docstring -%}
    ///     {{ docstring.as_comment() }}
    ///     {% endif -%}
    ///     {{ property.name }}: {{ property.ty.render(*namespace) }}
    ///     {%- endfor %}
    ///     {%- if class_.properties.is_empty() && class_.docstring.is_none() %}
    ///     pass
    ///     {% endif %}
    /// ```
    pub fn print(class_: &Class, namespace: Namespace) -> String;
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    /// Normalize whitespace: trim trailing whitespace from each line
    fn normalize(s: &str) -> String {
        s.lines().map(str::trim_end).collect::<Vec<_>>().join("\n")
    }

    macro_rules! test_class_render {
        (
            $test_name:ident:
            class $name:ident $(@ $doc:literal)? {
                $($prop_name:ident: $prop_ty:literal $(@ $pdoc:literal)?),* $(,)?
            }
            =>
            $expected:expr
        ) => {
            #[test]
            fn $test_name() {
                let class_ = Class::from_codegen_types(&baml_codegen_tests::class!(
                    $name $(@ $doc)? {
                        $($prop_name: $prop_ty $(@ $pdoc)?),*
                    }
                ));
                assert_eq!(
                    normalize(&print(&class_, Namespace::Types)),
                    normalize(crate::docstring::dedent($expected).trim())
                );
            }
        };
    }

    test_class_render! {
        class_with_docs:
        class Person @ "A person model" {
            name: "string" @ "The person's name",
            age: "int" @ "The person's age",
        }
        =>
        r#"
            class Person(BaseModel):
                """A person model"""

                # The person's name
                name: str
                # The person's age
                age: int
            "#
    }

    test_class_render! {
        class_no_docs:
        class User {
            email: "string",
            active: "bool",
        }
        =>
        r#"
            class User(BaseModel):
                email: str
                active: bool
            "#
    }

    test_class_render! {
        class_only_class_doc:
        class Config @ "Configuration settings" {
            timeout: "int",
            retries: "int",
        }
        =>
        r#"
            class Config(BaseModel):
                """Configuration settings"""
                
                timeout: int
                retries: int
            "#
    }

    test_class_render! {
        class_only_property_docs:
        class Request {
            url: "string" @ "The URL to request",
            method: "string",
            body: "string" @ "Request body content",
        }
        =>
        r#"
            class Request(BaseModel):
                # The URL to request
                url: str
                method: str
                # Request body content
                body: str
            "#
    }

    test_class_render! {
        class_multiline_docstring:
        class Animal @ "An animal model.\nUsed for classification." {
            species: "string" @ "The species name",
            weight: "float",
        }
        =>
        r#"
            class Animal(BaseModel):
                """
                An animal model.
                Used for classification.
                """
                
                # The species name
                species: str
                weight: float
            "#
    }

    test_class_render! {
        class_empty:
        class Empty {
        }
        =>
        r#"
            class Empty(BaseModel):
                pass
            "#
    }

    test_class_render! {
        class_empty_with_doc:
        class EmptyWithDoc @ "Empty class" {
        }
        =>
        r#"
            class EmptyWithDoc(BaseModel):
                """Empty class"""
            "#
    }

    test_class_render! {
        class_with_optional_type:
        class Profile {
            name: "string",
            bio: "string?",
        }
        =>
        r#"
            class Profile(BaseModel):
                name: str
                bio: typing.Optional[str]
            "#
    }

    test_class_render! {
        class_with_list_type:
        class Team {
            name: "string",
            members: "string[]",
        }
        =>
        r#"
            class Team(BaseModel):
                name: str
                members: typing.List[str]
            "#
    }

    test_class_render! {
        class_with_nested_class:
        class Order {
            id: "int",
            item: "Product",
        }
        =>
        r#"
            class Order(BaseModel):
                id: int
                item: Product
            "#
    }

    test_class_render! {
        class_with_complex_types:
        class Response {
            data: "string[]",
            errors: "string?",
        }
        =>
        r#"
            class Response(BaseModel):
                data: typing.List[str]
                errors: typing.Optional[str]
            "#
    }
}
