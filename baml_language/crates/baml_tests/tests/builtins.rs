//! Unified tests for built-in methods and operations.

use baml_tests::baml_test;
use bex_engine::BexExternalValue;

#[tokio::test]
async fn builtin_method_call() {
    let output = baml_test!(
        r#"
        function main() -> int {
            let arr = [1, 2, 3];
            arr.length()
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @"
    function main() -> int {
        load_const 1
        load_const 2
        load_const 3
        alloc_array 3
        call baml.Array.length
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(3)));
}

#[tokio::test]
async fn bind_method_call() {
    let output = baml_test!(
        r#"
        function main() -> int {
            let arr = [1, 2, 3];
            let v = arr.length();
            v
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @"
    function main() -> int {
        load_const 1
        load_const 2
        load_const 3
        alloc_array 3
        call baml.Array.length
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(3)));
}

#[tokio::test]
async fn any_value_to_string() {
    let output = baml_test!(
        r#"
        class Point {
            x int
            y int
        }

        class Person {
            name string
            age int
            location Point
            hobbies string[]
            scores map<string, int>
        }

        function main() -> string {
            let p = Point { x: 10, y: 20 };
            let person = Person {
                name: "Alice",
                age: 25,
                location: p,
                hobbies: ["reading", "coding"],
                scores: {"math": 95, "english": 88}
            };

            baml.unstable.string(person)
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> string {
        alloc_instance Person
        load_const "Alice"
        init_field .name
        load_const 25
        init_field .age
        alloc_instance Point
        load_const 10
        init_field .x
        load_const 20
        init_field .y
        init_field .location
        load_const "reading"
        load_const "coding"
        alloc_array 2
        init_field .hobbies
        load_const 95
        load_const 88
        load_const "math"
        load_const "english"
        alloc_map 2
        init_field .scores
        call baml.unstable.string
        return
    }
    "#);

    assert_eq!(
        output.result,
        Ok(BexExternalValue::String(
            r#"user.Person {
    name: "Alice"
    age: 25
    location: user.Point {
        x: 10
        y: 20
    }
    hobbies: ["reading", "coding"]
    scores: {
        "math": 95
        "english": 88
    }
}"#
            .to_string()
        ))
    );
}
