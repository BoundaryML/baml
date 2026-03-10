use super::*;
use crate::{baml_db, baml_tyannotated};

// type A = A[]
test_deserializer!(
    test_simple_recursive_alias_list,
    "[[], [], [[]]]",
    baml_tyannotated!([A]),
    baml_db! { type A = [A]; },
    [[], [], [[]]]
);

// type A = map<string, A>
test_deserializer!(
    test_simple_recursive_alias_map,
    r#"{"one": {"two": {}}, "three": {"four": {}}}"#,
    baml_tyannotated!(A),
    {
        let mut db = baml_db! {};
        db.try_add("A", crate::baml_tyresolved!(map<string, A>)).ok().unwrap();
        db
    },
    {
        "one": {"two": {}},
        "three": {"four": {}}
    }
);

// type A = map<string, A>, target = A | int
test_deserializer!(
    test_simple_recursive_alias_map_union,
    r#"{"one": {"two": {}}, "three": {"four": {}}}"#,
    baml_tyannotated!((A | int)),
    {
        let mut db = baml_db! {};
        db.try_add("A", crate::baml_tyresolved!(map<string, A>)).ok().unwrap();
        db
    },
    {
        "one": {"two": {}},
        "three": {"four": {}}
    }
);

// type A = B, type B = C, type C = A[]
// After inlining the chain: A = A[], B = A[], C = A[]
test_deserializer!(
    test_recursive_alias_cycle,
    "[[], [], [[]]]",
    baml_tyannotated!([A]),
    baml_db! { type A = [A]; type B = [A]; type C = [A]; },
    [[], [], [[]]]
);

/// Helper: build a TypeRefDb for the JsonValue type.
/// type JsonValue = int | float | bool | string | null | JsonValue[] | map<string, JsonValue>
///
/// We introduce helper aliases JsonValueArr and JsonValueMap because the
/// baml_db! type-alias syntax requires a single token-tree, which map<…>
/// cannot satisfy.
fn json_value_db() -> TypeRefDb<'static, &'static str> {
    baml_db! {
        type JsonValueArr = [JsonValue];
        type JsonValueMap = (map<string, JsonValue>);
        type JsonValue = (int | float | bool | string | null | JsonValueArr | JsonValueMap);
    }
}

test_deserializer!(
    test_json_without_nested_objects,
    r#"
    {
        "int": 1,
        "float": 1.0,
        "string": "test",
        "bool": true
    }
    "#,
    baml_tyannotated!(JsonValue),
    json_value_db(),
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
    {
        "number": 1,
        "string": "test",
        "bool": true,
        "list": [1, 2, 3]
    }
    "#,
    baml_tyannotated!(JsonValue),
    json_value_db(),
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
    baml_tyannotated!(JsonValue),
    json_value_db(),
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
    baml_tyannotated!(JsonValue),
    json_value_db(),
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
    baml_tyannotated!(JsonValue),
    json_value_db(),
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
    [[42.1]]
    "#,
    baml_tyannotated!(JsonValue),
    json_value_db(),
    [[42.1]]
);

/// Helper: build a TypeRefDb for JsonValue defined with cycles.
/// type JsonValue = int | float | bool | string | null | JsonArray | JsonObject
/// type JsonArray = JsonValue[]
/// type JsonObject = map<string, JsonValue>
fn json_value_with_cycles_db() -> TypeRefDb<'static, &'static str> {
    let mut db = baml_db! {
        type JsonArray = [JsonValue];
    };
    db.try_add(
        "JsonObject",
        crate::baml_tyresolved!(map<string, JsonValue>),
    )
    .ok()
    .unwrap();
    db.try_add(
        "JsonValue",
        crate::baml_tyresolved!((int | float | bool | string | null | JsonArray | JsonObject)),
    )
    .ok()
    .unwrap();
    db
}

test_deserializer!(
    test_json_defined_with_cycles,
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
    baml_tyannotated!(JsonValue),
    json_value_with_cycles_db(),
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
    baml_tyannotated!(JsonValue),
    json_value_db(),
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

// test_alias_with_class: Uses the cyclic JsonValue type with the recipe structure.
test_deserializer!(
    test_alias_with_class,
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
    baml_tyannotated!(JsonValue),
    json_value_with_cycles_db(),
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
