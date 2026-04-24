use crate::{objects::TypeAlias, ty::Namespace};

baml_codegen_types::render_fn! {
    /// ```askama
    /// {% if type_alias.is_recursive() -%}
    /// {{ type_alias.render_name(*namespace) }} = typing_extensions.TypeAliasType("{{ type_alias.render_name(*namespace) }}", {{ type_alias.render_rhs(*namespace) }})
    /// {% else -%}
    /// {{ type_alias.render_name(*namespace) }} = {{ type_alias.render_rhs(*namespace) }}
    /// {% endif %}
    /// ```
    pub fn print(type_alias: &TypeAlias, namespace: Namespace) -> String;
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    macro_rules! test_type_alias_render {
        (
            $test_name:ident:
            type $name:ident = $resolves_to:literal
            =>
            $expected:expr
        ) => {
            #[test]
            fn $test_name() {
                let type_alias = TypeAlias::from_codegen_types(&baml_codegen_tests::type_alias!(
                    $name = $resolves_to
                ));
                assert_eq!(print(&type_alias, Namespace::Types), $expected);
            }
        };
    }

    test_type_alias_render! {
        type_alias_to_string:
        type Name = "string"
        =>
        "Name = str"
    }

    test_type_alias_render! {
        type_alias_to_int:
        type Age = "int"
        =>
        "Age = int"
    }

    test_type_alias_render! {
        type_alias_to_float:
        type Score = "float"
        =>
        "Score = float"
    }

    test_type_alias_render! {
        type_alias_to_bool:
        type Flag = "bool"
        =>
        "Flag = bool"
    }

    test_type_alias_render! {
        type_alias_to_class:
        type PersonAlias = "Person"
        =>
        "PersonAlias = Person"
    }

    test_type_alias_render! {
        type_alias_to_optional:
        type MaybeName = "string?"
        =>
        "MaybeName = typing.Optional[str]"
    }

    test_type_alias_render! {
        type_alias_to_list:
        type Names = "string[]"
        =>
        "Names = typing.List[str]"
    }

    test_type_alias_render! {
        type_alias_to_optional_list:
        type MaybeNames = "string[]?"
        =>
        "MaybeNames = typing.Optional[typing.List[str]]"
    }

    test_type_alias_render! {
        type_alias_to_list_of_optional:
        type ListOfMaybeStrings = "string?[]"
        =>
        "ListOfMaybeStrings = typing.List[typing.Optional[str]]"
    }

    test_type_alias_render! {
        type_alias_to_nested_list:
        type Matrix = "int[][]"
        =>
        "Matrix = typing.List[typing.List[int]]"
    }

    test_type_alias_render! {
        type_alias_to_class_list:
        type People = "Person[]"
        =>
        "People = typing.List[Person]"
    }

    test_type_alias_render! {
        type_alias_to_optional_class:
        type MaybePerson = "Person?"
        =>
        "MaybePerson = typing.Optional[Person]"
    }

    test_type_alias_render! {
        type_alias_to_map:
        type Metadata = "map<string, string>"
        =>
        "Metadata = typing.Dict[str, str]"
    }

    test_type_alias_render! {
        type_alias_to_map_with_class_value:
        type PersonMap = "map<string, Person>"
        =>
        "PersonMap = typing.Dict[str, Person]"
    }

    test_type_alias_render! {
        type_alias_to_union:
        type StringOrInt = "string | int"
        =>
        "StringOrInt = typing.Union[str, int]"
    }

    test_type_alias_render! {
        type_alias_to_union_of_classes:
        type Animal = "Dog | Cat | Bird"
        =>
        "Animal = typing.Union[Dog, Cat, Bird]"
    }

    test_type_alias_render! {
        type_alias_complex_nested:
        type ComplexType = "map<string, Item?[]>"
        =>
        "ComplexType = typing.Dict[str, typing.List[typing.Optional[Item]]]"
    }

    #[test]
    fn type_alias_recursive_self_ref() {
        // RecursiveAlias = int | RecursiveAlias[]
        // Recursive type aliases use TypeAliasType for Pydantic v2 compatibility.
        // The self-reference inside the body should be quoted to avoid NameError.
        // Note: the test builder parses "int | RecursiveAlias[]" as List(Union(int, RecursiveAlias))
        // due to suffix-first parsing, so the generated form is List[Union[int, "RecursiveAlias"]].
        use baml_codegen_tests::ty;
        use baml_codegen_types::TypeAlias as CgTypeAlias;

        let type_alias = TypeAlias::from_codegen_types(&CgTypeAlias {
            name: baml_codegen_types::Name {
                pkg: "user".into(),
                namespace_path: vec![],
                name: "RecursiveAlias".into(),
            },
            resolves_to: ty("int | RecursiveAlias[]"),
            recursive: true,
        });
        let rendered = print(&type_alias, crate::ty::Namespace::Types);
        // The self-reference "RecursiveAlias" inside the body should be quoted
        assert!(
            rendered.contains("\"RecursiveAlias\""),
            "Expected quoted self-ref in: {rendered}"
        );
        // Recursive aliases use TypeAliasType for Pydantic v2 compatibility
        assert_eq!(
            rendered.trim(),
            r#"RecursiveAlias = typing_extensions.TypeAliasType("RecursiveAlias", typing.List[typing.Union[int, "RecursiveAlias"]])"#
        );
    }
}
