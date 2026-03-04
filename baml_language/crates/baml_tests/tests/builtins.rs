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

    insta::assert_snapshot!(output.bytecode, @r"
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

    insta::assert_snapshot!(output.bytecode, @r"
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
        copy 0
        load_const "Alice"
        store_field .name
        copy 0
        load_const 25
        store_field .age
        copy 0
        alloc_instance Point
        copy 0
        load_const 10
        store_field .x
        copy 0
        load_const 20
        store_field .y
        store_field .location
        copy 0
        load_const "reading"
        load_const "coding"
        alloc_array 2
        store_field .hobbies
        copy 0
        load_const 95
        load_const 88
        load_const "math"
        load_const "english"
        alloc_map 2
        store_field .scores
        call baml.unstable.string
        return
    }
    "#);

    assert_eq!(
        output.result,
        Ok(BexExternalValue::String(
            r#"Person {
    name: "Alice"
    age: 25
    location: Point {
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
