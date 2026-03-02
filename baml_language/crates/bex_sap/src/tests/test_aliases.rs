use super::*;

// type A = A[]
test_deserializer!(
    test_simple_recursive_alias_list,
    "[[], [], [[]]]",
    array_of(annotated(Ty::Unresolved("A"))),
    crate::baml_db! { type A = [A]; },
    [[], [], [[]]]
);

// type A = map<string, A>
test_deserializer!(
    test_simple_recursive_alias_map,
    r#"{"one": {"two": {}}, "three": {"four": {}}}"#,
    map_of(annotated(string_ty()), annotated(Ty::Unresolved("A"))),
    {
        let mut db = TypeRefDb::new();
        db.try_add("A", map_of(annotated(string_ty()), annotated(Ty::Unresolved("A")))).ok().unwrap();
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
    union_of(vec![
        annotated(Ty::Unresolved("A")),
        annotated(int_ty()),
    ]),
    {
        let mut db = TypeRefDb::new();
        db.try_add("A", map_of(annotated(string_ty()), annotated(Ty::Unresolved("A")))).ok().unwrap();
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
    array_of(annotated(Ty::Unresolved("A"))),
    crate::baml_db! { type A = [A]; type B = [A]; type C = [A]; },
    [[], [], [[]]]
);

/// Helper: build JsonValue type and db.
/// type JsonValue = int | float | bool | string | null | JsonValue[] | map<string, JsonValue>
fn json_value_db() -> (
    TyResolved<'static, &'static str>,
    TypeRefDb<'static, &'static str>,
) {
    let json_value_ty = union_of(vec![
        annotated(int_ty()),
        annotated(float_ty()),
        annotated(bool_ty()),
        annotated(string_ty()),
        annotated(null_ty()),
        annotated(array_of(annotated(Ty::Unresolved("JsonValue")))),
        annotated(map_of(
            annotated(string_ty()),
            annotated(Ty::Unresolved("JsonValue")),
        )),
    ]);
    let mut db = TypeRefDb::new();
    let target = json_value_ty.clone();
    db.try_add("JsonValue", json_value_ty).ok().unwrap();
    (target, db)
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
    json_value_db().0,
    json_value_db().1,
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
    json_value_db().0,
    json_value_db().1,
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
    json_value_db().0,
    json_value_db().1,
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
    json_value_db().0,
    json_value_db().1,
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
    json_value_db().0,
    json_value_db().1,
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
    json_value_db().0,
    json_value_db().1,
    [[42.1]]
);

/// Helper for JsonValue defined with cycles:
/// type JsonValue = int | float | bool | string | null | JsonArray | JsonObject
/// type JsonArray = JsonValue[]
/// type JsonObject = map<string, JsonValue>
fn json_value_with_cycles_db() -> (
    TyResolved<'static, &'static str>,
    TypeRefDb<'static, &'static str>,
) {
    let json_value_ty = union_of(vec![
        annotated(int_ty()),
        annotated(float_ty()),
        annotated(bool_ty()),
        annotated(string_ty()),
        annotated(null_ty()),
        annotated(Ty::Unresolved("JsonArray")),
        annotated(Ty::Unresolved("JsonObject")),
    ]);
    let mut db = TypeRefDb::new();
    db.try_add(
        "JsonArray",
        array_of(annotated(Ty::Unresolved("JsonValue"))),
    )
    .ok()
    .unwrap();
    db.try_add(
        "JsonObject",
        map_of(
            annotated(string_ty()),
            annotated(Ty::Unresolved("JsonValue")),
        ),
    )
    .ok()
    .unwrap();
    let target = json_value_ty.clone();
    db.try_add("JsonValue", json_value_ty).ok().unwrap();
    (target, db)
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
    json_value_with_cycles_db().0,
    json_value_with_cycles_db().1,
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
    json_value_db().0,
    json_value_db().1,
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
// The old test used load_test_ir and render_output_format which no longer exist.
// Converted to the standard pipeline with constructed types.
#[test]
fn test_alias_with_class() {
    let (target_ty, db) = json_value_with_cycles_db();

    let raw = r#"{
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
    }"#;

    let parsed = crate::jsonish::parse(raw, Default::default(), true).unwrap();
    let ctx = crate::deserializer::coercer::ParsingContext::new(target_ty.as_ref(), &db);
    let annotations = TypeAnnotations::default();
    let target = TyWithMeta::new(target_ty.as_ref(), &annotations);
    let result = TyResolvedRef::coerce(&ctx, target, &parsed);

    assert!(result.is_ok(), "Failed to parse: {result:?}");
    let value = result.unwrap();
    assert!(value.is_some(), "Coercion returned None");
}
