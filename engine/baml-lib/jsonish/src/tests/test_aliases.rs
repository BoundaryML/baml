use baml_types::{ir_type::UnionConstructor, type_meta::base::TypeMeta, LiteralValue};

use super::*;

test_deserializer!(
    test_simple_recursive_alias_list,
    r#"
type A = A[]
    "#,
    "[[], [], [[]]]",
    TypeIR::recursive_type_alias("A"),
    [[], [], [[]]]
);

test_deserializer!(
    test_simple_recursive_alias_map,
    r#"
type A = map<string, A>
    "#,
    r#"{"one": {"two": {}}, "three": {"four": {}}}"#,
    TypeIR::recursive_type_alias("A"),
    {
        "one": {"two": {}},
        "three": {"four": {}}
    }
);

test_deserializer!(
    test_simple_recursive_alias_map_union,
    r#"
type A = map<string, A>
    "#,
    r#"{"one": {"two": {}}, "three": {"four": {}}}"#,
    TypeIR::union(vec![
        TypeIR::recursive_type_alias("A"),
        TypeIR::int(),
    ]),
    {
        "one": {"two": {}},
        "three": {"four": {}}
    }
);

test_deserializer!(
    test_recursive_alias_cycle,
    r#"
type A = B
type B = C
type C = A[]
    "#,
    "[[], [], [[]]]",
    TypeIR::recursive_type_alias("A"),
    [[], [], [[]]]
);

test_deserializer!(
    test_json_without_nested_objects,
    r#"
type JsonValue = int | float | bool | string | null | JsonValue[] | map<string, JsonValue> 
    "#,
    r#"
    {
        "int": 1,
        "float": 1.0,
        "string": "test",
        "bool": true
    }
    "#,
    TypeIR::recursive_type_alias("JsonValue"),
    {
        "int": 1,
        "float": 1.0,
        "string": "test",
        "bool": true
    }
);

test_deserializer!(
    test_json_with_nested_list,
    r#"
type JsonValue = int | float | bool | string | null | JsonValue[] | map<string, JsonValue> 
    "#,
    r#"
    {
        "number": 1,
        "string": "test",
        "bool": true,
        "list": [1, 2, 3]
    }
    "#,
    TypeIR::recursive_type_alias("JsonValue"),
    {
        "number": 1,
        "string": "test",
        "bool": true,
        "list": [1, 2, 3]
    }
);

test_deserializer!(
    test_json_with_nested_object,
    r#"
type JsonValue = int | float | bool | string | null | JsonValue[] | map<string, JsonValue> 
    "#,
    r#"
    {
        "number": 1,
        "string": "test",
        "bool": true,
        "json": {
            "number": 1,
            "string": "test",
            "bool": true
        }
    }
    "#,
    TypeIR::recursive_type_alias("JsonValue"),
    {
        "number": 1,
        "string": "test",
        "bool": true,
        "json": {
            "number": 1,
            "string": "test",
            "bool": true
        }
    }
);

test_deserializer!(
    test_full_json_with_nested_objects,
    r#"
type JsonValue = int | float | bool | string | null | JsonValue[] | map<string, JsonValue> 
    "#,
    r#"
    {
        "number": 1,
        "string": "test",
        "bool": true,
        "list": [1, 2, 3],
        "object": {
            "number": 1,
            "string": "test",
            "bool": true,
            "list": [1, 2, 3]
        },
        "json": {
            "number": 1,
            "string": "test",
            "bool": true,
            "list": [1, 2, 3],
            "object": {
                "number": 1,
                "string": "test",
                "bool": true,
                "list": [1, 2, 3]
            }
        }
    }
    "#,
    TypeIR::recursive_type_alias("JsonValue"),
    {
        "number": 1,
        "string": "test",
        "bool": true,
        "list": [1, 2, 3],
        "object": {
            "number": 1,
            "string": "test",
            "bool": true,
            "list": [1, 2, 3]
        },
        "json": {
            "number": 1,
            "string": "test",
            "bool": true,
            "list": [1, 2, 3],
            "object": {
                "number": 1,
                "string": "test",
                "bool": true,
                "list": [1, 2, 3]
            }
        }
    }
);

test_deserializer!(
    test_list_of_json_objects,
    r#"
type JsonValue = int | float | bool | string | null | JsonValue[] | map<string, JsonValue> 
    "#,
    r#"
    [
        {
            "number": 1,
            "string": "test",
            "bool": true,
            "list": [1, 2, 3]
        },
        {
            "number": 1,
            "string": "test",
            "bool": true,
            "list": [1, 2, 3]
        }
    ]
    "#,
    TypeIR::recursive_type_alias("JsonValue"),
    [
        {
            "number": 1,
            "string": "test",
            "bool": true,
            "list": [1, 2, 3]
        },
        {
            "number": 1,
            "string": "test",
            "bool": true,
            "list": [1, 2, 3]
        }
    ]
);

test_deserializer!(
    test_nested_list,
    r#"
type JsonValue = int | float | bool | string | null | JsonValue[] | map<string, JsonValue> 
    "#,
    r#"
    [[42.1]]
    "#,
    TypeIR::recursive_type_alias("JsonValue"),
    // [[[[[[[[[[[[[[[[[[[[42]]]]]]]]]]]]]]]]]]]]
    [[42.1]]
);

test_deserializer!(
    test_json_defined_with_cycles,
    r#"
type JsonValue = int | float | bool | string | null | JsonArray | JsonObject
type JsonArray = JsonValue[]
type JsonObject = map<string, JsonValue>
    "#,
    r#"
    {
        "number": 1,
        "string": "test",
        "bool": true,
        "json": {
            "number": 1,
            "string": "test",
            "bool": true
        }
    }
    "#,
    TypeIR::recursive_type_alias("JsonValue"),
    {
        "number": 1,
        "string": "test",
        "bool": true,
        "json": {
            "number": 1,
            "string": "test",
            "bool": true
        }
    }
);

test_deserializer!(
    test_ambiguous_int_string_json_type,
    r#"
type JsonValue = int | float | bool | string | null | JsonValue[] | map<string, JsonValue> 
    "#,
    r#"
    {
        "recipe": {
            "name": "Chocolate Chip Cookies",
            "servings": 24,
            "ingredients": [
                "2 1/4 cups all-purpose flour", "1/2 teaspoon baking soda",
                "1 cup unsalted butter, room temperature",
                "1/2 cup granulated sugar",
                "1 cup packed light-brown sugar",
                "1 teaspoon salt", "2 teaspoons pure vanilla extract",
                "2 large eggs", "2 cups semisweet and/or milk chocolate chips"
            ],
            "instructions": [
                "Preheat oven to 350°F (180°C).",
                "In a small bowl, whisk together flour and baking soda; set aside.",
                "In a large bowl, cream butter and sugars until light and fluffy.",
                "Add salt, vanilla, and eggs; mix well.",
                "Gradually stir in flour mixture.",
                "Fold in chocolate chips.",
                "Drop by rounded tablespoons onto ungreased baking sheets.",
                "Bake for 10-12 minutes or until golden brown.",
                "Cool on wire racks."
            ]
        }
    }
    "#,
    TypeIR::recursive_type_alias("JsonValue"),
    {
        "recipe": {
            "name": "Chocolate Chip Cookies",
            "servings": 24,
            "ingredients": [
                "2 1/4 cups all-purpose flour", "1/2 teaspoon baking soda",
                "1 cup unsalted butter, room temperature",
                "1/2 cup granulated sugar",
                "1 cup packed light-brown sugar",
                "1 teaspoon salt", "2 teaspoons pure vanilla extract",
                "2 large eggs", "2 cups semisweet and/or milk chocolate chips"
            ],
            "instructions": [
                "Preheat oven to 350°F (180°C).",
                "In a small bowl, whisk together flour and baking soda; set aside.",
                "In a large bowl, cream butter and sugars until light and fluffy.",
                "Add salt, vanilla, and eggs; mix well.",
                "Gradually stir in flour mixture.",
                "Fold in chocolate chips.",
                "Drop by rounded tablespoons onto ungreased baking sheets.",
                "Bake for 10-12 minutes or until golden brown.",
                "Cool on wire racks."
            ]
        }
    }
);

#[test_log::test]
fn test_alias_with_class() {
    let ir = crate::helpers::load_test_ir(
        r#"
type JsonValue = int | float | bool | string | null | JsonArray | JsonObject
type JsonArray = JsonValue[]
type JsonObject = map<string, JsonValue>
    "#,
    );

    let target_type = TypeIR::recursive_type_alias("JsonValue");
    let target = crate::helpers::render_output_format(
        &ir,
        &target_type,
        &Default::default(),
        baml_types::StreamingMode::NonStreaming,
    )
    .unwrap();

    let result = from_str(
        &target,
        &target_type,
        r#"{
        "recipe": {
            "name": "Chocolate Chip Cookies",
            "servings": 24,
            "ingredients": [
                "2 1/4 cups all-purpose flour", "1/2 teaspoon baking soda",
                "1 cup unsalted butter, room temperature",
                "1/2 cup granulated sugar",
                "1 cup packed light-brown sugar",
                "1 teaspoon salt", "2 teaspoons pure vanilla extract",
                "2 large eggs", "2 cups semisweet and/or milk chocolate chips"
            ],
            "instructions": [
                "Preheat oven to 350°F (180°C).",
                "In a small bowl, whisk together flour and baking soda; set aside.",
                "In a large bowl, cream butter and sugars until light and fluffy.",
                "Add salt, vanilla, and eggs; mix well.",
                "Gradually stir in flour mixture.",
                "Fold in chocolate chips.",
                "Drop by rounded tablespoons onto ungreased baking sheets.",
                "Bake for 10-12 minutes or until golden brown.",
                "Cool on wire racks."
            ]
        }
    }"#,
        true,
    );

    assert!(result.is_ok(), "Failed to parse: {result:?}");

    let value = result.unwrap();

    assert_eq!(value.field_type(), &target_type);
}
