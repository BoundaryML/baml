use super::*;

// --- Foo: class with string list ---
// class Foo { hi string[] }
// class Bar { foo string }

test_deserializer!(
    test_foo,
    r#"{"hi": ["a", "b"]}"#,
    class_ty("Foo", vec![field("hi", array_of(annotated(string_ty())))]),
    empty_db(),
    {"hi": ["a", "b"]}
);

#[test]
fn test_wrapped_objects() {
    let db = crate::baml_db! {
        class Foo { hi: [string] }
    };
    let target_ty = array_of(annotated(Ty::Unresolved("Foo")));

    let raw = r#"{"hi": "a"}"#;
    let parsed = crate::jsonish::parse(raw, Default::default(), true).unwrap();
    let ctx = crate::deserializer::coercer::ParsingContext::new(target_ty.as_ref(), &db);
    let annots = TypeAnnotations::default();
    let target = TyWithMeta::new(target_ty.as_ref(), &annots);
    let result = TyResolvedRef::coerce(&ctx, target, &parsed);
    assert!(result.is_ok());
    let value = result.unwrap().unwrap();
    let json_value = serde_json::to_value(&value).unwrap();
    let expected = serde_json::json!([{"hi": ["a"]}]);
    assert_eq!(json_value, expected);
}

test_deserializer!(
    test_string_from_obj_and_string,
    r#"The output is: {"hi": ["a", "b"]}"#,
    class_ty("Foo", vec![field("hi", array_of(annotated(string_ty())))]),
    empty_db(),
    {"hi": ["a", "b"]}
);

test_deserializer!(
    test_string_from_obj_and_string_with_extra_text,
    r#"This is a test. The output is: {"hi": ["a", "b"]}"#,
    class_ty("Foo", vec![field("hi", array_of(annotated(string_ty())))]),
    empty_db(),
    {"hi": ["a", "b"]}
);

test_deserializer!(
    test_string_from_obj_and_string_with_invalid_extra_text,
    r#"{"hi": ["a", "b"]} is the output."#,
    class_ty("Foo", vec![field("hi", array_of(annotated(string_ty())))]),
    empty_db(),
    {"hi": ["a", "b"]}
);

test_deserializer!(
    str_with_quotes,
    r#"{"foo": "[\"bar\"]"}"#,
    class_ty("Bar", vec![field("foo", string_ty())]),
    empty_db(),
    {"foo": "[\"bar\"]"}
);

test_deserializer!(
    str_with_nested_json,
    r#"{"foo": "{\"foo\": [\"bar\"]}"}"#,
    class_ty("Bar", vec![field("foo", string_ty())]),
    empty_db(),
    {"foo": "{\"foo\": [\"bar\"]}"}
);

test_deserializer!(
    test_obj_from_str_with_string_foo,
    r#"
{
  "foo": "Here is how you can build the API call:\n```json\n{\n  \"foo\": {\n    \"world\": [\n      \"bar\"\n    ]\n  }\n}\n```"
}
"#,
    class_ty("Bar", vec![field("foo", string_ty())]),
    empty_db(),
    {"foo": "Here is how you can build the API call:\n```json\n{\n  \"foo\": {\n    \"world\": [\n      \"bar\"\n    ]\n  }\n}\n```"}
);

// --- Optional Foo ---
// class Foo { foo string? }

test_deserializer!(
    test_optional_foo,
    r#"{}"#,
    class_ty("Foo", vec![optional_field("foo", string_ty())]),
    empty_db(),
    { "foo": null }
);

test_deserializer!(
    test_optional_foo_with_value,
    r#"{"foo": ""}"#,
    class_ty("Foo", vec![optional_field("foo", string_ty())]),
    empty_db(),
    { "foo": "" }
);

// --- Multi-fielded Foo ---
// class Foo { one string, two string? }

test_deserializer!(
    test_multi_fielded_foo,
    r#"{"one": "a"}"#,
    class_ty("Foo", vec![
        field("one", string_ty()),
        optional_field("two", string_ty()),
    ]),
    empty_db(),
    { "one": "a", "two": null }
);

test_deserializer!(
    test_multi_fielded_foo_with_optional,
    r#"{"one": "a", "two": "b"}"#,
    class_ty("Foo", vec![
        field("one", string_ty()),
        optional_field("two", string_ty()),
    ]),
    empty_db(),
    { "one": "a", "two": "b" }
);

test_deserializer!(
    test_multi_fielded_foo_with_optional_and_extra_text,
    r#"Here is how you can build the API call:
    ```json
    {
        "one": "hi",
        "two": "hello"
    }
    ```

    ```json
        {
            "test2": {
                "key2": "value"
            },
            "test21": [
            ]
        }
    ```"#,
    class_ty("Foo", vec![
        field("one", string_ty()),
        optional_field("two", string_ty()),
    ]),
    empty_db(),
    { "one": "hi", "two": "hello" }
);

// --- Multi-fielded Foo with list ---
// class Foo { a int, b string, c string[] }

test_deserializer!(
    test_multi_fielded_foo_with_list,
    r#"{"a": 1, "b": "hi", "c": ["a", "b"]}"#,
    class_ty("Foo", vec![
        field("a", int_ty()),
        field("b", string_ty()),
        field("c", array_of(annotated(string_ty()))),
    ]),
    empty_db(),
    { "a": 1, "b": "hi", "c": ["a", "b"] }
);

// --- Nested class ---
// class Foo { a string }
// class Bar { foo Foo }

#[test]
fn test_nested_class() {
    let db = crate::baml_db! {
        class Foo { a: string }
        class Bar { foo: Foo }
    };

    let raw = r#"{"foo": {"a": "hi"}}"#;
    let target_ty = db.resolved_from_ident(&"Bar").unwrap();
    let parsed = crate::jsonish::parse(raw, Default::default(), true).unwrap();
    let ctx = crate::deserializer::coercer::ParsingContext::new(target_ty, &db);
    let annots = TypeAnnotations::default();
    let target = TyWithMeta::new(target_ty, &annots);
    let result = TyResolvedRef::coerce(&ctx, target, &parsed);
    assert!(result.is_ok());
    let value = result.unwrap().unwrap();
    let json_value = serde_json::to_value(&value).unwrap();
    let expected = serde_json::json!({ "foo": { "a": "hi" } });
    assert_eq!(json_value, expected);
}

#[test]
fn test_nested_class_with_extra_text() {
    let db = crate::baml_db! {
        class Foo { a: string }
        class Bar { foo: Foo }
    };

    let raw = r#"Here is how you can build the API call:
    ```json
    {
        "foo": {
            "a": "hi"
        }
    }
    ```

    and this
    ```json
    {
        "foo": {
            "a": "twooo"
        }
    }"#;
    let target_ty = db.resolved_from_ident(&"Bar").unwrap();
    let parsed = crate::jsonish::parse(raw, Default::default(), true).unwrap();
    let ctx = crate::deserializer::coercer::ParsingContext::new(target_ty, &db);
    let annots = TypeAnnotations::default();
    let target = TyWithMeta::new(target_ty, &annots);
    let result = TyResolvedRef::coerce(&ctx, target, &parsed);
    assert!(result.is_ok());
    let value = result.unwrap().unwrap();
    let json_value = serde_json::to_value(&value).unwrap();
    let expected = serde_json::json!({ "foo": { "a": "hi" } });
    assert_eq!(json_value, expected);
}

#[test]
fn test_nested_class_with_prefix() {
    let db = crate::baml_db! {
        class Foo { a: string }
        class Bar { foo: Foo }
    };

    let raw = r#"Here is how you can build the API call:
    {
        "foo": {
            "a": "hi"
        }
    }

    and this
    {
        "foo": {
            "a": "twooo"
        }
    }
    "#;
    let target_ty = db.resolved_from_ident(&"Bar").unwrap();
    let parsed = crate::jsonish::parse(raw, Default::default(), true).unwrap();
    let ctx = crate::deserializer::coercer::ParsingContext::new(target_ty, &db);
    let annots = TypeAnnotations::default();
    let target = TyWithMeta::new(target_ty, &annots);
    let result = TyResolvedRef::coerce(&ctx, target, &parsed);
    assert!(result.is_ok());
    let value = result.unwrap().unwrap();
    let json_value = serde_json::to_value(&value).unwrap();
    let expected = serde_json::json!({ "foo": { "a": "hi" } });
    assert_eq!(json_value, expected);
}

// --- Resume ---
// class Resume { name string, email string?, phone string?, experience string[], education string[], skills string[] }

test_deserializer!(
    test_resume,
    r#"{
        "name": "Lee Hsien Loong",
        "email": null,
        "phone": null,
        "experience": [
            "Senior Minister of Singapore since 2024",
            "Prime Minister of Singapore from 2004 to 2024",
            "Member of Parliament (MP) for the Teck Ghee division of Ang Mo Kio GRC since 1991",
            "Teck Ghee SMC between 1984 and 1991",
            "Secretary-General of the People's Action Party (PAP) since 2004"
        ],
        "education": [],
        "skills": ["politician", "former brigadier-general"]
    }"#,
    class_ty("Resume", vec![
        field("name", string_ty()),
        optional_field("email", string_ty()),
        optional_field("phone", string_ty()),
        field("experience", array_of(annotated(string_ty()))),
        field("education", array_of(annotated(string_ty()))),
        field("skills", array_of(annotated(string_ty()))),
    ]),
    empty_db(),
    {
        "name": "Lee Hsien Loong",
        "email": null,
        "phone": null,
        "experience": [
            "Senior Minister of Singapore since 2024",
            "Prime Minister of Singapore from 2004 to 2024",
            "Member of Parliament (MP) for the Teck Ghee division of Ang Mo Kio GRC since 1991",
            "Teck Ghee SMC between 1984 and 1991",
            "Secretary-General of the People's Action Party (PAP) since 2004"
        ],
        "education": [],
        "skills": ["politician", "former brigadier-general"]
    }
);

test_partial_deserializer!(
    test_resume_partial,
    r#"{
        "name": "Lee Hsien Loong",
        "email": null,
        "phone": null,
        "experience": [
            "Senior Minister of Singapore since 2024",
            "Prime Minister of Singapore from 2004 to "#,
    class_ty("Resume", vec![
        field("name", string_ty()),
        optional_field("email", string_ty()),
        optional_field("phone", string_ty()),
        field("experience", array_of(annotated(string_ty()))),
        field("education", array_of(annotated(string_ty()))),
        field("skills", array_of(annotated(string_ty()))),
    ]),
    empty_db(),
    {
        "name": "Lee Hsien Loong",
        "email": null,
        "phone": null,
        "experience": [
            "Senior Minister of Singapore since 2024",
            "Prime Minister of Singapore from 2004 to "
        ],
        "education": [],
        "skills": []
    }
);

test_partial_deserializer!(
    test_resume_partial_2,
    r#"{
        "experience": [
            "Senior Minister of Singapore since 2024",
            "Prime Minister of Singapore from 2004 to "#,
    class_ty("Resume", vec![
        field("name", string_ty()),
        optional_field("email", string_ty()),
        optional_field("phone", string_ty()),
        field("experience", array_of(annotated(string_ty()))),
        field("education", array_of(annotated(string_ty()))),
        field("skills", array_of(annotated(string_ty()))),
    ]),
    empty_db(),
    {
        "name": null,
        "email": null,
        "phone": null,
        "experience": [
            "Senior Minister of Singapore since 2024",
            "Prime Minister of Singapore from 2004 to "
        ],
        "education": [],
        "skills": []
    }
);

// --- Class with aliases ---
// class TestClassAlias {
//     key string @alias("key-dash")
//     key2 string @alias("key21")
//     key3 string @alias("key with space")
//     key4 string //unaliased
//     key5 string @alias("key.with.punctuation/123")
// }

test_deserializer!(
    test_aliases,
    r#"{
        "key-dash": "This is a value with a dash",
        "key21": "This is a value for key21",
        "key with space": "This is a value with space",
        "key4": "This is a value for key4",
        "key.with.punctuation/123": "This is a value with punctuation and numbers"
      }"#,
    class_ty("TestClassAlias", vec![
        field_with_aliases("key", string_ty(), vec!["key-dash"]),
        field_with_aliases("key2", string_ty(), vec!["key21"]),
        field_with_aliases("key3", string_ty(), vec!["key with space"]),
        field("key4", string_ty()),
        field_with_aliases("key5", string_ty(), vec!["key.with.punctuation/123"]),
    ]),
    empty_db(),
    {
        "key": "This is a value with a dash",
        "key2": "This is a value for key21",
        "key3": "This is a value with space",
        "key4": "This is a value for key4",
        "key5": "This is a value with punctuation and numbers"
    }
);

// --- Simple class with nested class ---
// class SimpleTest { answer Answer }
// class Answer { content float }

#[test]
fn test_class_with_whitespace_keys() {
    let db = crate::baml_db! {
        class Answer { content: float }
        class SimpleTest { answer: Answer }
    };

    let raw = r#"{" answer ": {" content ": 78.54}}"#;
    let target_ty = db.resolved_from_ident(&"SimpleTest").unwrap();
    let parsed = crate::jsonish::parse(raw, Default::default(), true).unwrap();
    let ctx = crate::deserializer::coercer::ParsingContext::new(target_ty, &db);
    let annots = TypeAnnotations::default();
    let target = TyWithMeta::new(target_ty, &annots);
    let result = TyResolvedRef::coerce(&ctx, target, &parsed);
    assert!(result.is_ok());
    let value = result.unwrap().unwrap();
    let json_value = serde_json::to_value(&value).unwrap();
    let expected = serde_json::json!({
        "answer": {
            "content": 78.54
        }
    });
    assert_eq!(json_value, expected);
}

// --- Class with nested class list ---
// class Resume { name string, education Education[], skills string[] }
// class Education { school string, degree string, year int }

#[test]
fn test_class_with_nested_list() {
    let db = crate::baml_db! {
        class Education {
            school: string,
            degree: string,
            year: int
        }
        class Resume {
            name: string,
            education: [Education],
            skills: [string]
        }
    };

    let raw = r#"{
        "name": "Vaibhav Gupta",
        "education": [
            {
                "school": "FOOO",
                "degree": "FOOO",
                "year": 2015
            },
            {
                "school": "BAAR",
                "degree": "BAAR",
                "year": 2019
            }
        ],
        "skills": [
          "C++",
          "SIMD on custom silicon"
        ]
      }"#;
    let parsed = crate::jsonish::parse(raw, Default::default(), true).unwrap();
    let ctx = crate::deserializer::coercer::ParsingContext::new(
        db.resolved_from_ident(&"Resume").unwrap(),
        &db,
    );
    let annots = TypeAnnotations::default();
    let target = TyWithMeta::new(db.resolved_from_ident(&"Resume").unwrap(), &annots);
    let value = TyResolvedRef::coerce(&ctx, target, &parsed)
        .unwrap()
        .unwrap();
    let json_value = serde_json::to_value(&value).unwrap();
    let expected = serde_json::json!({
        "name": "Vaibhav Gupta",
        "education": [
            {
                "school": "FOOO",
                "degree": "FOOO",
                "year": 2015
            },
            {
                "school": "BAAR",
                "degree": "BAAR",
                "year": 2019
            }
        ],
        "skills": [
            "C++",
            "SIMD on custom silicon"
        ]
    });
    assert_eq!(json_value, expected);
}

#[test]
fn test_class_with_nestedd_list_just_list() {
    let db = crate::baml_db! {
        class Education { school: string, degree: string, year: int }
    };
    let target_ty = array_of(annotated(Ty::Unresolved("Education")));

    let raw = r#"[
          {
            "school": "FOOO",
            "degree": "FOOO",
            "year": 2015
          },
          {
            "school": "BAAR",
            "degree": "BAAR",
            "year": 2019
          }
        ]
    "#;
    let parsed = crate::jsonish::parse(raw, Default::default(), true).unwrap();
    let ctx = crate::deserializer::coercer::ParsingContext::new(target_ty.as_ref(), &db);
    let annots = TypeAnnotations::default();
    let target = TyWithMeta::new(target_ty.as_ref(), &annots);
    let result = TyResolvedRef::coerce(&ctx, target, &parsed);
    assert!(result.is_ok());
    let value = result.unwrap().unwrap();
    let json_value = serde_json::to_value(&value).unwrap();
    let expected = serde_json::json!([
        {
            "school": "FOOO",
            "degree": "FOOO",
            "year": 2015
        },
        {
            "school": "BAAR",
            "degree": "BAAR",
            "year": 2019
        }
    ]);
    assert_eq!(json_value, expected);
}

// --- Function classes with union ---
// class Function { selected (Function1 | Function2 | Function3) }
// class Function1 { function_name string, radius int }
// class Function2 { function_name string, diameter int }
// class Function3 { function_name string, length int, breadth int }

fn make_function_db() -> TypeRefDb<'static, &'static str> {
    crate::baml_db! {
        class Function1 { function_name: string, radius: int }
        class Function2 { function_name: string, diameter: int }
        class Function3 { function_name: string, length: int, breadth: int }
        class Function { selected: (Function1 | Function2 | Function3) }
    }
}

#[test]
fn test_obj_created_when_not_present() {
    let db = make_function_db();
    let target_ty = array_of(annotated(Ty::Unresolved("Function")));

    let raw = r#"[
        {
          // Calculate the area of a circle based on the radius.
          function_name: 'circle.calculate_area',
          // The radius of the circle.
          radius: 5,
        },
        {
          // Calculate the circumference of a circle based on the diameter.
          function_name: 'circle.calculate_circumference',
          // The diameter of the circle.
          diameter: 10,
        }
      ]"#;
    let parsed = crate::jsonish::parse(raw, Default::default(), true).unwrap();
    let ctx = crate::deserializer::coercer::ParsingContext::new(target_ty.as_ref(), &db);
    let annots = TypeAnnotations::default();
    let target = TyWithMeta::new(target_ty.as_ref(), &annots);
    let result = TyResolvedRef::coerce(&ctx, target, &parsed);
    assert!(result.is_ok());
    let value = result.unwrap().unwrap();
    let json_value = serde_json::to_value(&value).unwrap();
    let expected = serde_json::json!([
        {"selected": {
            "function_name": "circle.calculate_area",
            "radius": 5
        }},
        {"selected": {
            "function_name": "circle.calculate_circumference",
            "diameter": 10
        }}
    ]);
    assert_eq!(json_value, expected);
}

#[test]
fn test_trailing_comma_with_space_last_field() {
    let fn2 = class_ty(
        "Function2",
        vec![
            field("function_name", string_ty()),
            field("diameter", int_ty()),
        ],
    );

    let raw = r#"
    {
      // Calculate the circumference of a circle based on the diameter.
      function_name: 'circle.calculate_circumference',
      // The diameter of the circle. (with a ", ")
      diameter: 10,
    }
    "#;
    let parsed = crate::jsonish::parse(raw, Default::default(), true).unwrap();
    let db = empty_db();
    let ctx = crate::deserializer::coercer::ParsingContext::new(fn2.as_ref(), &db);
    let annots = TypeAnnotations::default();
    let target = TyWithMeta::new(fn2.as_ref(), &annots);
    let result = TyResolvedRef::coerce(&ctx, target, &parsed);
    assert!(result.is_ok());
    let value = result.unwrap().unwrap();
    let json_value = serde_json::to_value(&value).unwrap();
    let expected = serde_json::json!({
        "function_name": "circle.calculate_circumference",
        "diameter": 10
    });
    assert_eq!(json_value, expected);
}

#[test]
fn test_trailing_comma_with_space_last_field_and_extra_text() {
    let fn2 = class_ty(
        "Function2",
        vec![
            field("function_name", string_ty()),
            field("diameter", int_ty()),
        ],
    );

    let raw = r#"
    {
      // Calculate the circumference of a circle based on the diameter.
      function_name: 'circle.calculate_circumference',
      // The diameter of the circle. (with a ", ")
      diameter: 10,
      Some key: "Some value"
    }
    and this
    "#;
    let parsed = crate::jsonish::parse(raw, Default::default(), true).unwrap();
    let db = empty_db();
    let ctx = crate::deserializer::coercer::ParsingContext::new(fn2.as_ref(), &db);
    let annots = TypeAnnotations::default();
    let target = TyWithMeta::new(fn2.as_ref(), &annots);
    let result = TyResolvedRef::coerce(&ctx, target, &parsed);
    assert!(result.is_ok());
    let value = result.unwrap().unwrap();
    let json_value = serde_json::to_value(&value).unwrap();
    let expected = serde_json::json!({
        "function_name": "circle.calculate_circumference",
        "diameter": 10
    });
    assert_eq!(json_value, expected);
}

// --- Nested obj from string fails ---
// class Foo { foo Bar }
// class Bar { bar string, option int? }

#[test]
fn test_nested_obj_from_string_fails_0() {
    let bar_cls = class_ty(
        "Bar",
        vec![
            field("bar", string_ty()),
            optional_field("option", int_ty()),
        ],
    );
    let foo_cls = class_ty("Foo", vec![field("foo", Ty::Unresolved("Bar"))]);
    let mut db = TypeRefDb::new();
    assert!(db.try_add("Bar", bar_cls).is_ok());

    let raw = r#"My inner string"#;
    let parsed = crate::jsonish::parse(raw, Default::default(), true).unwrap();
    let ctx = crate::deserializer::coercer::ParsingContext::new(foo_cls.as_ref(), &db);
    let annots = TypeAnnotations::default();
    let target = TyWithMeta::new(foo_cls.as_ref(), &annots);
    let result = TyResolvedRef::coerce(&ctx, target, &parsed);
    match result {
        Ok(Some(v)) => {
            let json = serde_json::to_value(&v).unwrap();
            panic!("Parsing should have failed, got: {json}");
        }
        Ok(None) => {}
        Err(_) => {}
    }
}

#[test]
fn test_nested_obj_from_string_fails_1() {
    let db = crate::baml_db! {
        class Bar { bar: string }
        class Foo { foo: Bar }
    };

    let raw = r#"My inner string"#;
    let target_ty = db.resolved_from_ident(&"Foo").unwrap();
    let parsed = crate::jsonish::parse(raw, Default::default(), true).unwrap();
    let ctx = crate::deserializer::coercer::ParsingContext::new(target_ty, &db);
    let annots = TypeAnnotations::default();
    let target = TyWithMeta::new(target_ty, &annots);
    let result = TyResolvedRef::coerce(&ctx, target, &parsed);
    match result {
        Ok(Some(v)) => {
            let json = serde_json::to_value(&v).unwrap();
            panic!("Parsing should have failed, got: {json}");
        }
        Ok(None) => {}
        Err(_) => {}
    }
}

test_failing_deserializer!(
    test_nested_obj_from_string_fails_2,
    r#"My inner string"#,
    class_ty("Foo", vec![field("foo", string_ty())]),
    empty_db()
);

test_deserializer!(
    test_nested_obj_from_int,
    r#"1214"#,
    class_ty("Foo", vec![field("foo", int_ty())]),
    empty_db(),
    { "foo": 1214 }
);

test_deserializer!(
    test_nested_obj_from_float,
    r#"1214.123"#,
    class_ty("Foo", vec![field("foo", float_ty())]),
    empty_db(),
    { "foo": 1214.123 }
);

test_deserializer!(
    test_nested_obj_from_bool,
    r#" true "#,
    class_ty("Foo", vec![field("foo", bool_ty())]),
    empty_db(),
    { "foo": true }
);

// --- Nested classes with aliases ---
// class Nested { prop3 string|null, prop4 string|null @alias("blah"), prop20 Nested2 }
// class Nested2 { prop11 string|null, prop12 string|null @alias("blah") }
// class Schema { prop1 string|null, prop2 Nested|string, prop5 (string|null)[], prop6 string|Nested[] @alias("blah"), nested_attrs (string|null|Nested)[], parens (string|null), other_group (string|(int|string)) @alias(other) }

#[test]
fn test_nested_classes_with_aliases() {
    let nested2_cls = class_ty(
        "Nested2",
        vec![
            optional_field("prop11", string_ty()),
            optional_field_with_aliases("prop12", string_ty(), vec!["blah"]),
        ],
    );
    let nested_cls = class_ty(
        "Nested",
        vec![
            optional_field("prop3", string_ty()),
            optional_field_with_aliases("prop4", string_ty(), vec!["blah"]),
            field("prop20", Ty::Unresolved("Nested2")),
        ],
    );

    let schema_cls = class_ty(
        "Schema",
        vec![
            optional_field("prop1", string_ty()),
            field(
                "prop2",
                union_of(vec![
                    annotated(Ty::Unresolved("Nested")),
                    annotated(string_ty()),
                ]),
            ),
            field("prop5", array_of(annotated(optional(string_ty())))),
            field_with_aliases(
                "prop6",
                union_of(vec![
                    annotated(string_ty()),
                    annotated(array_of(annotated(Ty::Unresolved("Nested")))),
                ]),
                vec!["blah"],
            ),
            field(
                "nested_attrs",
                array_of(annotated(union_of(vec![
                    annotated(string_ty()),
                    annotated(null_ty()),
                    annotated(Ty::Unresolved("Nested")),
                ]))),
            ),
            optional_field("parens", string_ty()),
            field_with_aliases(
                "other_group",
                union_of(vec![
                    annotated(string_ty()),
                    annotated(union_of(vec![annotated(int_ty()), annotated(string_ty())])),
                ]),
                vec!["other"],
            ),
        ],
    );

    let mut db = TypeRefDb::new();
    assert!(db.try_add("Nested2", nested2_cls).is_ok());
    assert!(db.try_add("Nested", nested_cls).is_ok());

    let raw = r#"
```json
{
  "prop1": "one",
  "prop2": {
    "prop3": "three",
    "blah": "four",
    "prop20": {
      "prop11": "three",
      "blah": "four"
    }
  },
  "prop5": ["hi"],
  "blah": "blah",
  "nested_attrs": ["nested"],
  "parens": "parens1",
  "other": "other"
}
```
"#;
    let parsed = crate::jsonish::parse(raw, Default::default(), true).unwrap();
    let ctx = crate::deserializer::coercer::ParsingContext::new(schema_cls.as_ref(), &db);
    let annots = TypeAnnotations::default();
    let target = TyWithMeta::new(schema_cls.as_ref(), &annots);
    let result = TyResolvedRef::coerce(&ctx, target, &parsed);
    assert!(result.is_ok());
    let value = result.unwrap().unwrap();
    let json_value = serde_json::to_value(&value).unwrap();
    let expected = serde_json::json!({
        "prop1": "one",
        "prop2": {
          "prop3": "three",
          "prop4": "four",
          "prop20": {
            "prop11": "three",
            "prop12": "four"
          }
        },
        "prop5": ["hi"],
        "prop6": "blah",
        "nested_attrs": ["nested"],
        "parens": "parens1",
        "other_group": "other"
    });
    assert_eq!(json_value, expected);
}

// --- Notion Page test (test_ekinsdrow) ---
// This is a large test with many interconnected types for Notion blocks.

#[test]
fn test_ekinsdrow() {
    // Enum types
    let column_type_enum = enum_ty(
        "ColumnType",
        vec![variant_with_aliases("Column", vec!["column"])],
    );
    let breadcrumb_type_enum = enum_ty(
        "BreadcrumbType",
        vec![variant_with_aliases("Breadcrumb", vec!["breadcrumb"])],
    );
    let column_list_type_enum = enum_ty(
        "ColumnListType",
        vec![variant_with_aliases("ColumnList", vec!["column_list"])],
    );
    let heading3_type_enum = enum_ty(
        "Heading3Type",
        vec![variant_with_aliases("Heading3", vec!["heading_3"])],
    );
    let paragraph_type_enum = enum_ty(
        "ParagraphType",
        vec![variant_with_aliases("Paragraph", vec!["paragraph"])],
    );
    let rich_text_type_enum = enum_ty(
        "RichTextType",
        vec![variant_with_aliases("RichText", vec!["text"])],
    );
    let todo_type_enum = enum_ty(
        "ToDoType",
        vec![variant_with_aliases("ToDo", vec!["to_do"])],
    );

    // Class types
    let rich_text_content_cls = class_ty("RichTextContent", vec![field("content", string_ty())]);
    let rich_text_cls = class_ty(
        "RichText",
        vec![
            field("type", Ty::Unresolved("RichTextType")),
            field("text", Ty::Unresolved("RichTextContent")),
        ],
    );
    let heading_body_cls = class_ty(
        "HeadingBody",
        vec![
            field("rich_text", array_of(annotated(Ty::Unresolved("RichText")))),
            field("is_toggleable", bool_ty()),
        ],
    );
    let heading3_cls = class_ty(
        "Heading3",
        vec![
            field("type", Ty::Unresolved("Heading3Type")),
            field("heading_3", Ty::Unresolved("HeadingBody")),
        ],
    );
    let paragraph_body_cls = class_ty(
        "ParagraphBody",
        vec![
            field("rich_text", array_of(annotated(Ty::Unresolved("RichText")))),
            field("children", array_of(annotated(string_ty()))),
        ],
    );
    let paragraph_cls = class_ty(
        "Paragraph",
        vec![
            field("type", Ty::Unresolved("ParagraphType")),
            field("paragraph", Ty::Unresolved("ParagraphBody")),
        ],
    );
    let todo_body_cls = class_ty(
        "ToDoBody",
        vec![
            field("rich_text", array_of(annotated(Ty::Unresolved("RichText")))),
            optional_field("checked", bool_ty()),
            field("children", array_of(annotated(Ty::Unresolved("Paragraph")))),
        ],
    );
    let todo_cls = class_ty(
        "ToDo",
        vec![
            field("type", Ty::Unresolved("ToDoType")),
            field("to_do", Ty::Unresolved("ToDoBody")),
        ],
    );
    let breadcrumb_cls = class_ty(
        "Breadcrumb",
        vec![
            field("type", Ty::Unresolved("BreadcrumbType")),
            field(
                "breadcrumb",
                map_of(annotated(string_ty()), annotated(string_ty())),
            ),
        ],
    );
    let breadcrumb1_cls = class_ty(
        "Breadcrumb1",
        vec![
            field("type", Ty::Unresolved("BreadcrumbType")),
            field(
                "breadcrumb",
                map_of(annotated(string_ty()), annotated(string_ty())),
            ),
        ],
    );
    let column_body_cls = class_ty(
        "ColumnBody",
        vec![field(
            "children",
            array_of(annotated(union_of(vec![
                annotated(Ty::Unresolved("Breadcrumb1")),
                annotated(Ty::Unresolved("Heading3")),
                annotated(Ty::Unresolved("Paragraph")),
                annotated(Ty::Unresolved("ToDo")),
            ]))),
        )],
    );
    let column_cls = class_ty(
        "Column",
        vec![
            field("type", Ty::Unresolved("ColumnType")),
            field("column", Ty::Unresolved("ColumnBody")),
        ],
    );
    let column_list_body_cls = class_ty(
        "ColumnListBody",
        vec![field(
            "children",
            array_of(annotated(Ty::Unresolved("Column"))),
        )],
    );
    let column_list_cls = class_ty(
        "ColumnList",
        vec![
            field("type", Ty::Unresolved("ColumnListType")),
            field("column_list", Ty::Unresolved("ColumnListBody")),
        ],
    );
    let icon_cls = class_ty("Icon", vec![field("emoji", string_ty())]);
    let page_cls = class_ty(
        "Page",
        vec![
            field("object", string_ty()),
            field("icon", Ty::Unresolved("Icon")),
            field(
                "children",
                array_of(annotated(union_of(vec![
                    annotated(Ty::Unresolved("Breadcrumb")),
                    annotated(Ty::Unresolved("ColumnList")),
                    annotated(Ty::Unresolved("Heading3")),
                    annotated(Ty::Unresolved("Paragraph")),
                    annotated(Ty::Unresolved("ToDo")),
                ]))),
            ),
        ],
    );

    let mut db = TypeRefDb::new();
    assert!(db.try_add("ColumnType", column_type_enum).is_ok());
    assert!(db.try_add("BreadcrumbType", breadcrumb_type_enum).is_ok());
    assert!(db.try_add("ColumnListType", column_list_type_enum).is_ok());
    assert!(db.try_add("Heading3Type", heading3_type_enum).is_ok());
    assert!(db.try_add("ParagraphType", paragraph_type_enum).is_ok());
    assert!(db.try_add("RichTextType", rich_text_type_enum).is_ok());
    assert!(db.try_add("ToDoType", todo_type_enum).is_ok());
    assert!(db.try_add("RichTextContent", rich_text_content_cls).is_ok());
    assert!(db.try_add("RichText", rich_text_cls).is_ok());
    assert!(db.try_add("HeadingBody", heading_body_cls).is_ok());
    assert!(db.try_add("Heading3", heading3_cls).is_ok());
    assert!(db.try_add("ParagraphBody", paragraph_body_cls).is_ok());
    assert!(db.try_add("Paragraph", paragraph_cls).is_ok());
    assert!(db.try_add("ToDoBody", todo_body_cls).is_ok());
    assert!(db.try_add("ToDo", todo_cls).is_ok());
    assert!(db.try_add("Breadcrumb", breadcrumb_cls).is_ok());
    assert!(db.try_add("Breadcrumb1", breadcrumb1_cls).is_ok());
    assert!(db.try_add("ColumnBody", column_body_cls).is_ok());
    assert!(db.try_add("Column", column_cls).is_ok());
    assert!(db.try_add("ColumnListBody", column_list_body_cls).is_ok());
    assert!(db.try_add("ColumnList", column_list_cls).is_ok());
    assert!(db.try_add("Icon", icon_cls).is_ok());

    let raw = r#"{
  "object": "page",
  "icon": {
    "emoji": "\u{1F4DA}"
  },
  "children": [
    {
      "type": "column_list",
      "column_list": {
        "children": [
          {
            "type": "column",
            "column": {
              "children": [
                {
                  "type": "heading_3",
                  "heading_3": {
                    "rich_text": [
                      {
                        "type": "text",
                        "text": {
                          "content": "The Lord of the Rings"
                        }
                      }
                    ],
                    "is_toggleable": false
                  }
                },
                {
                  "type": "paragraph",
                  "paragraph": {
                    "rich_text": [
                      {
                        "type": "text",
                        "text": {
                          "content": "J.R.R. Tolkien"
                        }
                      }
                    ]
                  }
                },
                {
                  "type": "to_do",
                  "to_do": {
                    "rich_text": [
                      {
                        "type": "text",
                        "text": {
                          "content": "Read again"
                        }
                      }
                    ],
                    "checked": false
                  }
                }
              ]
            }
          }
        ]
      }
    }
  ]
}"#;
    let parsed = crate::jsonish::parse(raw, Default::default(), true).unwrap();
    let ctx = crate::deserializer::coercer::ParsingContext::new(page_cls.as_ref(), &db);
    let annots = TypeAnnotations::default();
    let target = TyWithMeta::new(page_cls.as_ref(), &annots);
    let result = TyResolvedRef::coerce(&ctx, target, &parsed);
    assert!(result.is_ok());
    let value = result.unwrap().unwrap();
    let json_value = serde_json::to_value(&value).unwrap();
    let expected = serde_json::json!({
        "object": "page",
        "icon": {
          "emoji": "\u{1F4DA}"
        },
        "children": [
          {
            "type": "ColumnList",
            "column_list": {
              "children": [
                {
                  "type": "Column",
                  "column": {
                    "children": [
                      {
                        "type": "Heading3",
                        "heading_3": {
                          "rich_text": [
                            {
                              "type": "RichText",
                              "text": {
                                "content": "The Lord of the Rings"
                              }
                            }
                          ],
                          "is_toggleable": false
                        }
                      },
                      {
                        "type": "Paragraph",
                        "paragraph": {
                          "rich_text": [
                            {
                              "type": "RichText",
                              "text": {
                                "content": "J.R.R. Tolkien"
                              }
                            }
                          ],
                        "children": []
                        }
                      },
                      {
                        "type": "ToDo",
                        "to_do": {
                          "rich_text": [
                            {
                              "type": "RichText",
                              "text": {
                                "content": "Read again"
                              }
                            }
                          ],
                          "checked": false,
                        "children": []
                        }
                      }
                    ]
                  }
                }
              ]
            }
          }
        ]
    });
    assert_eq!(json_value, expected);
}

// --- Escaped quotes test ---
// class DoCommandACReturnType { sections (TextSection | CodeSection)[] }
// class TextSection { text string }
// class CodeSection { code_language string, code string }

#[test]
fn test_escaped_quotes() {
    let db = crate::baml_db! {
        class TextSection { text: string }
        class CodeSection { code_language: string, code: string }
        class DoCommandACReturnType { sections: [(TextSection | CodeSection)] }
    };

    let raw = r#"
Certainly! I'll redesign the UI to make it more appealing to a female audience. I'll focus on color schemes, fonts, and imagery that are generally more attractive to women. Here's my thought process and suggestions:

Thoughts: "The current design is quite neutral. We can make it more feminine by using softer colors, curved shapes, and adding some playful elements. We should also consider updating the trending items to be more relevant to a female audience."

"We can use a pastel color scheme, which is often associated with femininity. Let's go with a soft pink as the primary color, with accents of lavender and mint green."

"For the font, we can use a more elegant and rounded typeface for the logo and headings. This will give a softer, more feminine look."

"We should update the trending items to include more fashion-focused and accessory items that are popular among women."

Here's the redesigned code with these changes:

{
  "sections": [
    {
      "code_language": "swift",
      "code": "import SwiftUI\n\nstruct ContentView: View {\n    var body: some View {\n        ZStack(alignment: .bottom) {\n            VStack(spacing: 0) {\n                CustomNavigationBar()\n                \n                ScrollView {\n                    VStack(spacing: 20) {\n                        LogoSection()\n                        TrendingSection()\n                    }\n                    .padding()\n                }\n            }\n            .background(Color(\"SoftPink\")) // Change background to soft pink\n            \n            BottomSearchBar()\n        }\n        .edgesIgnoringSafeArea(.bottom)\n    }\n}\n"
    },
    {
      "text": "To complete this redesign, you'll need to add some custom colors to your asset catalog."
    }
  ]

  "#;
    let target_ty = db.resolved_from_ident(&"DoCommandACReturnType").unwrap();
    let parsed = crate::jsonish::parse(raw, Default::default(), true).unwrap();
    let ctx = crate::deserializer::coercer::ParsingContext::new(target_ty, &db);
    let annots = TypeAnnotations::default();
    let target = TyWithMeta::new(target_ty, &annots);
    let result = TyResolvedRef::coerce(&ctx, target, &parsed);
    assert!(result.is_ok());
    let value = result.unwrap().unwrap();
    let json_value = serde_json::to_value(&value).unwrap();
    let expected = serde_json::json!({
        "sections": [
            {
                "code_language": "swift",
                "code": "import SwiftUI\n\nstruct ContentView: View {\n    var body: some View {\n        ZStack(alignment: .bottom) {\n            VStack(spacing: 0) {\n                CustomNavigationBar()\n                \n                ScrollView {\n                    VStack(spacing: 20) {\n                        LogoSection()\n                        TrendingSection()\n                    }\n                    .padding()\n                }\n            }\n            .background(Color(\"SoftPink\")) // Change background to soft pink\n            \n            BottomSearchBar()\n        }\n        .edgesIgnoringSafeArea(.bottom)\n    }\n}\n"
            },
            {
                "text": "To complete this redesign, you'll need to add some custom colors to your asset catalog."
            }
        ]
    });
    assert_eq!(json_value, expected);
}

// --- Object stream test ---
// class Foo { a int, c int, b int }

test_partial_deserializer!(
    test_object_finished_ints,
    r#"{"a": 1234,"b": 1234, "c": 1234}"#,
    class_ty("Foo", vec![
        field("a", int_ty()),
        field("c", int_ty()),
        field("b", int_ty()),
    ]),
    empty_db(),
    {"a": 1234, "b": 1234, "c": 1234}
);

// --- Empty string value ---

test_deserializer!(
    test_empty_string_value,
    r#"{"a": ""}"#,
    class_ty("Foo", vec![field("a", string_ty())]),
    empty_db(),
    {"a": ""}
);

test_deserializer!(
    test_empty_string_value_1,
    r#"{a: ""}"#,
    class_ty("Foo", vec![field("a", string_ty())]),
    empty_db(),
    {"a": ""}
);

test_deserializer!(
    test_empty_string_value_2,
    r#"{
    a: "",
    b: "",
    res: []
  }"#,
    class_ty("Foo", vec![
        field("a", string_ty()),
        field("b", string_ty()),
        field("res", array_of(annotated(string_ty()))),
    ]),
    empty_db(),
    {"a": "", "b": "", "res": []}
);

test_deserializer!(
    test_string_field_with_spaces,
    r#"{
    a: Hi friends!,
    b: hey world lets do something kinda cool
    so that we can test this out,
    res: [hello,
     world]
  }"#,
    class_ty("Foo", vec![
        field("a", string_ty()),
        field("b", string_ty()),
        field("res", array_of(annotated(string_ty()))),
    ]),
    empty_db(),
    {
        "a": "Hi friends!",
        "b": "hey world lets do something kinda cool\n    so that we can test this out",
        "res": ["hello", "world"]
    }
);

// --- Recursive type ---
// class Foo { pointer Foo? }

#[test]
fn test_recursive_type() {
    // For a self-recursive type, we use the db with Ty::Unresolved
    let real_foo = class_ty(
        "Foo",
        vec![AnnotatedField {
            name: std::borrow::Cow::Borrowed("pointer"),
            ty: annotated(union_of(vec![
                annotated(Ty::Unresolved("Foo")),
                annotated(null_ty()),
            ])),
            class_in_progress_field_missing: AttrLiteral::Null,
            class_completed_field_missing: AttrLiteral::Null,
            aliases: vec![],
        }],
    );
    let mut db = TypeRefDb::new();
    assert!(db.try_add("Foo", real_foo.clone()).is_ok());

    let raw = r#"
    The answer is
    {
      "pointer": {
        "pointer": null
      }
    },

    Anything else I can help with?
  "#;
    let parsed = crate::jsonish::parse(raw, Default::default(), true).unwrap();
    let ctx = crate::deserializer::coercer::ParsingContext::new(real_foo.as_ref(), &db);
    let annots = TypeAnnotations::default();
    let target = TyWithMeta::new(real_foo.as_ref(), &annots);
    let result = TyResolvedRef::coerce(&ctx, target, &parsed);
    assert!(result.is_ok());
    let value = result.unwrap().unwrap();
    let json_value = serde_json::to_value(&value).unwrap();
    let expected = serde_json::json!({
        "pointer": {
            "pointer": null,
        },
    });
    assert_eq!(json_value, expected);
}

#[test]
fn test_recursive_type_missing_brackets_and_quotes() {
    let real_foo = class_ty(
        "Foo",
        vec![AnnotatedField {
            name: std::borrow::Cow::Borrowed("pointer"),
            ty: annotated(union_of(vec![
                annotated(Ty::Unresolved("Foo")),
                annotated(null_ty()),
            ])),
            class_in_progress_field_missing: AttrLiteral::Null,
            class_completed_field_missing: AttrLiteral::Null,
            aliases: vec![],
        }],
    );
    let mut db = TypeRefDb::new();
    assert!(db.try_add("Foo", real_foo.clone()).is_ok());

    let raw = r#"
    The answer is
    {
      "pointer": {
        pointer: null,

    Anything else I can help with?
  "#;
    let parsed = crate::jsonish::parse(raw, Default::default(), true).unwrap();
    let ctx = crate::deserializer::coercer::ParsingContext::new(real_foo.as_ref(), &db);
    let annots = TypeAnnotations::default();
    let target = TyWithMeta::new(real_foo.as_ref(), &annots);
    let result = TyResolvedRef::coerce(&ctx, target, &parsed);
    assert!(result.is_ok());
    let value = result.unwrap().unwrap();
    let json_value = serde_json::to_value(&value).unwrap();
    let expected = serde_json::json!({
        "pointer": {
            "pointer": null,
        },
    });
    assert_eq!(json_value, expected);
}

// --- Recursive type with union ---
// class Foo { pointer Foo | int }

#[test]
fn test_recursive_type_with_union() {
    let db = crate::baml_db! {
        class Foo { pointer: (Foo | int) }
    };

    let raw = r#"
    The answer is
    {
      "pointer": {
        "pointer": 1,
      }
    },

    Anything else I can help with?
  "#;
    let target_ty = db.resolved_from_ident(&"Foo").unwrap();
    let parsed = crate::jsonish::parse(raw, Default::default(), true).unwrap();
    let ctx = crate::deserializer::coercer::ParsingContext::new(target_ty, &db);
    let annots = TypeAnnotations::default();
    let target = TyWithMeta::new(target_ty, &annots);
    let result = TyResolvedRef::coerce(&ctx, target, &parsed);
    assert!(result.is_ok());
    let value = result.unwrap().unwrap();
    let json_value = serde_json::to_value(&value).unwrap();
    let expected = serde_json::json!({
        "pointer": {
            "pointer": 1,
        },
    });
    assert_eq!(json_value, expected);
}

// --- Mutually recursive ---
// class Foo { b Bar | int }
// class Bar { f Foo | int }

#[test]
fn test_mutually_recursive_with_union() {
    let db = crate::baml_db! {
        class Foo { b: (Bar | int) }
        class Bar { f: (Foo | int) }
    };

    let raw = r#"
    The answer is
    {
      "b": {
        "f": {
          "b": 1
        },
      }
    },

    Anything else I can help with?
  "#;
    let target_ty = db.resolved_from_ident(&"Foo").unwrap();
    let parsed = crate::jsonish::parse(raw, Default::default(), true).unwrap();
    let ctx = crate::deserializer::coercer::ParsingContext::new(target_ty, &db);
    let annots = TypeAnnotations::default();
    let target = TyWithMeta::new(target_ty, &annots);
    let result = TyResolvedRef::coerce(&ctx, target, &parsed);
    assert!(result.is_ok());
    let value = result.unwrap().unwrap();
    let json_value = serde_json::to_value(&value).unwrap();
    let expected = serde_json::json!({
        "b": {
            "f": {
                "b": 1,
            },
        },
    });
    assert_eq!(json_value, expected);
}

#[test]
fn test_recursive_type_with_union_missing_brackets_and_quotes() {
    let db = crate::baml_db! {
        class Foo { pointer: (Foo | int) }
    };

    let raw = r#"
    The answer is
    {
      "pointer": {
        pointer: 1
    },

    Anything else I can help with?
  "#;
    let target_ty = db.resolved_from_ident(&"Foo").unwrap();
    let parsed = crate::jsonish::parse(raw, Default::default(), true).unwrap();
    let ctx = crate::deserializer::coercer::ParsingContext::new(target_ty, &db);
    let annots = TypeAnnotations::default();
    let target = TyWithMeta::new(target_ty, &annots);
    let result = TyResolvedRef::coerce(&ctx, target, &parsed);
    assert!(result.is_ok());
    let value = result.unwrap().unwrap();
    let json_value = serde_json::to_value(&value).unwrap();
    let expected = serde_json::json!({
        "pointer": {
            "pointer": 1,
        },
    });
    assert_eq!(json_value, expected);
}

// --- Recursive union on multiple fields ---
// class Foo { rec_one Foo | int, rec_two Foo | int }

#[test]
fn test_recursive_union_on_multiple_fields_single_line() {
    let db = crate::baml_db! {
        class Foo { rec_one: (Foo | int), rec_two: (Foo | int) }
    };

    let raw = r#"
    The answer is
    {
      "rec_one": { "rec_one": 1, "rec_two": 2 },
      "rec_two": {
        "rec_one": { "rec_one": 1, "rec_two": 2 },
        "rec_two": { "rec_one": 1, "rec_two": 2 }
      }
    },

    Anything else I can help with?
  "#;
    let target_ty = db.resolved_from_ident(&"Foo").unwrap();
    let parsed = crate::jsonish::parse(raw, Default::default(), true).unwrap();
    let ctx = crate::deserializer::coercer::ParsingContext::new(target_ty, &db);
    let annots = TypeAnnotations::default();
    let target = TyWithMeta::new(target_ty, &annots);
    let result = TyResolvedRef::coerce(&ctx, target, &parsed);
    assert!(result.is_ok());
    let value = result.unwrap().unwrap();
    let json_value = serde_json::to_value(&value).unwrap();
    let expected = serde_json::json!({
        "rec_one": {
            "rec_one": 1,
            "rec_two": 2
        },
        "rec_two": {
            "rec_one": {
                "rec_one": 1,
                "rec_two": 2
            },
            "rec_two": {
                "rec_one": 1,
                "rec_two": 2
            }
        },
    });
    assert_eq!(json_value, expected);
}

#[test]
fn test_recursive_union_on_multiple_fields_single_line_without_quotes() {
    let db = crate::baml_db! {
        class Foo { rec_one: (Foo | int), rec_two: (Foo | int) }
    };

    let raw = r#"
    The answer is
    {
      rec_one: { rec_one: 1, rec_two: 2 },
      rec_two: {
        rec_one: { rec_one: 1, rec_two: 2 },
        rec_two: { rec_one: 1, rec_two: 2 }
      }
    },

    Anything else I can help with?
  "#;
    let target_ty = db.resolved_from_ident(&"Foo").unwrap();
    let parsed = crate::jsonish::parse(raw, Default::default(), true).unwrap();
    let ctx = crate::deserializer::coercer::ParsingContext::new(target_ty, &db);
    let annots = TypeAnnotations::default();
    let target = TyWithMeta::new(target_ty, &annots);
    let result = TyResolvedRef::coerce(&ctx, target, &parsed);
    assert!(result.is_ok());
    let value = result.unwrap().unwrap();
    let json_value = serde_json::to_value(&value).unwrap();
    let expected = serde_json::json!({
        "rec_one": {
            "rec_one": 1,
            "rec_two": 2
        },
        "rec_two": {
            "rec_one": {
                "rec_one": 1,
                "rec_two": 2
            },
            "rec_two": {
                "rec_one": 1,
                "rec_two": 2
            }
        },
    });
    assert_eq!(json_value, expected);
}

// --- Recursive single line with bool ---
// class Foo { rec_one Foo | int | bool, rec_two Foo | int | bool }

#[test]
fn test_recursive_single_line() {
    let db = crate::baml_db! {
        class Foo { rec_one: (Foo | int | bool), rec_two: (Foo | int | bool) }
    };

    let raw = r#"
    The answer is
    { rec_one: true, rec_two: false },

    Anything else I can help with?
  "#;
    let target_ty = db.resolved_from_ident(&"Foo").unwrap();
    let parsed = crate::jsonish::parse(raw, Default::default(), true).unwrap();
    let ctx = crate::deserializer::coercer::ParsingContext::new(target_ty, &db);
    let annots = TypeAnnotations::default();
    let target = TyWithMeta::new(target_ty, &annots);
    let result = TyResolvedRef::coerce(&ctx, target, &parsed);
    assert!(result.is_ok());
    let value = result.unwrap().unwrap();
    let json_value = serde_json::to_value(&value).unwrap();
    let expected = serde_json::json!({
        "rec_one": true,
        "rec_two": false
    });
    assert_eq!(json_value, expected);
}

// --- Complex recursive union ---
// class Foo { rec_one Foo | int | bool, rec_two Foo | int | bool | null }

#[test]
fn test_recursive_union_on_multiple_fields_single_line_without_quotes_complex() {
    let db = crate::baml_db! {
        class Foo { rec_one: (Foo | int | bool), rec_two: (Foo | int | bool | null) }
    };

    let raw = r#"
    The answer is
    {
      rec_one: { rec_one: { rec_one: true, rec_two: false }, rec_two: null },
      rec_two: {
        rec_one: { rec_one: { rec_one: 1, rec_two: 2 }, rec_two: null },
        rec_two: { rec_one: 1, rec_two: null }
      }
    },

    Anything else I can help with?
  "#;
    let target_ty = db.resolved_from_ident(&"Foo").unwrap();
    let parsed = crate::jsonish::parse(raw, Default::default(), true).unwrap();
    let ctx = crate::deserializer::coercer::ParsingContext::new(target_ty, &db);
    let annots = TypeAnnotations::default();
    let target = TyWithMeta::new(target_ty, &annots);
    let result = TyResolvedRef::coerce(&ctx, target, &parsed);
    assert!(result.is_ok());
    let value = result.unwrap().unwrap();
    let json_value = serde_json::to_value(&value).unwrap();
    let expected = serde_json::json!({
        "rec_one": {
            "rec_one": {
                "rec_one": true,
                "rec_two": false
            },
            "rec_two": null
        },
        "rec_two": {
            "rec_one": {
                "rec_one": {
                    "rec_one": 1,
                    "rec_two": 2
                },
                "rec_two": null
            },
            "rec_two": {
                "rec_one": 1,
                "rec_two": null
            }
        },
    });
    assert_eq!(json_value, expected);
}

// --- String in object with unescaped quotes ---

test_deserializer!(
    test_string_in_object_with_unescaped_quotes,
    r#"
    The answer is
    { rec_one: "and then i said \"hi\", and also \"bye\"", rec_two: "and then i said "hi", and also "bye"", "also_rec_one": ok },

    Anything else I can help with?
  "#,
    class_ty("Foo", vec![
        field("rec_one", string_ty()),
        field("rec_two", string_ty()),
        field("also_rec_one", string_ty()),
    ]),
    empty_db(),
    {
        "rec_one": "and then i said \"hi\", and also \"bye\"",
        "rec_two": "and then i said \"hi\", and also \"bye\"",
        "also_rec_one": "ok"
    }
);

// --- Array in object ---

test_deserializer!(
    test_array_in_object,
    r#"
    The answer is
    { rec_one: ["first with "quotes", and also "more"", "second"], rec_two: ["third", "fourth"] },
  "#,
    class_ty("Foo", vec![
        field("rec_one", array_of(annotated(string_ty()))),
        field("rec_two", array_of(annotated(string_ty()))),
    ]),
    empty_db(),
    {
        "rec_one": vec!["first with \"quotes\", and also \"more\"", "second"],
        "rec_two": vec!["third", "fourth"]
    }
);

// --- Enum without leading newline ---
// class WithFoo { foo Foo, name string }
// enum Foo { FOO, BAR }

#[test]
fn test_enum_without_leading_newline() {
    let db = crate::baml_db! {
        enum Foo { FOO, BAR }
        class WithFoo { foo: Foo, name: string }
    };

    let raw = r#"
    {foo:FOO, name: "Greg"}
  "#;
    let target_ty = db.resolved_from_ident(&"WithFoo").unwrap();
    let parsed = crate::jsonish::parse(raw, Default::default(), true).unwrap();
    let ctx = crate::deserializer::coercer::ParsingContext::new(target_ty, &db);
    let annots = TypeAnnotations::default();
    let target = TyWithMeta::new(target_ty, &annots);
    let result = TyResolvedRef::coerce(&ctx, target, &parsed);
    assert!(result.is_ok());
    let value = result.unwrap().unwrap();
    let json_value = serde_json::to_value(&value).unwrap();
    let expected = serde_json::json!({
        "foo": "FOO",
        "name": "Greg"
    });
    assert_eq!(json_value, expected);
}

// --- Aliases with capitalized key ---

test_deserializer!(
    test_aliases_with_capitalized_key,
    // Key21 is now capitalized, but we should still be able to parse it.
    r#"{
      "key-dash": "This is a value with a dash",
      "Key21": "This is a value for key21",
      "key with space": "This is a value with space",
      "key4": "This is a value for key4",
      "key.with.punctuation/123": "This is a value with punctuation and numbers"
    }"#,
    class_ty("TestClassAlias", vec![
        field_with_aliases("key", string_ty(), vec!["key-dash"]),
        field_with_aliases("key2", string_ty(), vec!["key21"]),
        field_with_aliases("key3", string_ty(), vec!["key with space"]),
        field("key4", string_ty()),
        field_with_aliases("key5", string_ty(), vec!["key.with.punctuation/123"]),
    ]),
    empty_db(),
    {
        "key": "This is a value with a dash",
        "key2": "This is a value for key21",
        "key3": "This is a value with space",
        "key4": "This is a value for key4",
        "key5": "This is a value with punctuation and numbers"
    }
);

// --- Class with capitalization ---

#[test]
fn test_class_with_capitalization() {
    let db = crate::baml_db! {
        class Answer { content: float }
        class SimpleTest { answer: Answer }
    };

    let raw = r#"{"Answer": {" content ": 78.54}}"#;
    let target_ty = db.resolved_from_ident(&"SimpleTest").unwrap();
    let parsed = crate::jsonish::parse(raw, Default::default(), true).unwrap();
    let ctx = crate::deserializer::coercer::ParsingContext::new(target_ty, &db);
    let annots = TypeAnnotations::default();
    let target = TyWithMeta::new(target_ty, &annots);
    let result = TyResolvedRef::coerce(&ctx, target, &parsed);
    assert!(result.is_ok());
    let value = result.unwrap().unwrap();
    let json_value = serde_json::to_value(&value).unwrap();
    let expected = serde_json::json!({
        "answer": {
            "content": 78.54
        }
    });
    assert_eq!(json_value, expected);
}

// --- Skip field ---
// class SkipField { dont_skip string, skip_this_one string? @skip }
// @skip doesn't map to sap_model, so we model it as optional_field

test_deserializer!(
    test_skip_field,
    r#"{"dont_skip": "ok"}"#,
    class_ty("SkipField", vec![
        field("dont_skip", string_ty()),
        optional_field("skip_this_one", string_ty()),
    ]),
    empty_db(),
    {
        "dont_skip": "ok",
        "skip_this_one": null
    }
);

// Skipped tests:
// - test_optional_list (uses test_partial_deserializer_streaming! which doesn't exist)
// - test_integ_test_failure (uses test_partial_deserializer_streaming!)
// - test_string_in_object_with_unescaped_quotes_2 (uses test_partial_deserializer_streaming!)
// - test_string_in_object_with_unescaped_quotes_3 (uses test_partial_deserializer_streaming!)
// - test_partial_array_in_object (uses test_partial_deserializer_streaming!)
// - test_array_with_unescaped_quotes (uses test_partial_deserializer_streaming!)
