use crate::{objects::TypeAlias, ty::Namespace};

baml_codegen_types::render_fn! {
    /// ```askama
    /// {{ type_alias.name.render(*namespace) }} = {{ type_alias.resolves_to.render(*namespace) }}
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
}
