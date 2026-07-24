//! Runtime semantics of array rest-pattern bindings: `..let r`
//! binds a copy of the unmatched middle of the scrutinee, typed `elem[]`.

use baml_tests::baml_test;
use bex_engine::BexExternalValue;

#[tokio::test]
async fn rest_binds_middle_values() {
    let output = baml_test!(
        r#"
        function main() -> int {
            match ([1, 2, 3, 4, 5]) {
                [let a, ..let r, let z] => r[0] * 100 + r[1] * 10 + r[2] + a * 1000 + z * 10000,
                _ => -1
            }
        }
        "#
    );
    // a=1, r=[2,3,4], z=5
    assert_eq!(output.result, Ok(BexExternalValue::Int(51234)));
}

#[tokio::test]
async fn rest_with_no_suffix_takes_tail() {
    let output = baml_test!(
        r#"
        function main() -> int {
            match ([7, 8, 9]) {
                [let a, ..let r] => r.length() * 10 + r[0] - a,
                _ => -1
            }
        }
        "#
    );
    // r=[8,9]: 2*10 + 8 - 7
    assert_eq!(output.result, Ok(BexExternalValue::Int(21)));
}

#[tokio::test]
async fn rest_is_empty_when_nothing_remains() {
    let output = baml_test!(
        r#"
        function main() -> int {
            match ([1]) {
                [let a, ..let r] => r.length(),
                _ => -1
            }
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(0)));
}

#[tokio::test]
async fn rest_is_empty_between_exact_prefix_and_suffix() {
    let output = baml_test!(
        r#"
        function main() -> int {
            match ([1, 2]) {
                [let a, ..let r, let z] => r.length() * 100 + a * 10 + z,
                _ => -1
            }
        }
        "#
    );
    // r=[], a=1, z=2
    assert_eq!(output.result, Ok(BexExternalValue::Int(12)));
}

#[tokio::test]
async fn too_short_array_falls_through_rest_arm() {
    let output = baml_test!(
        r#"
        function main() -> int {
            match ([9]) {
                [let a, ..let r, let z] => 1,
                _ => 2
            }
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(2)));
}

#[tokio::test]
async fn rest_only_copies_whole_array() {
    let output = baml_test!(
        r#"
        function main() -> int {
            let [..let all] = [1, 2, 3]
            return all.length() * 100 + all[0] * 10 + all[2]
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(313)));
}

#[tokio::test]
async fn pushing_to_rest_does_not_mutate_source() {
    let output = baml_test!(
        r#"
        function main() -> int {
            let xs = [1, 2, 3];
            match (xs) {
                [let a, ..let r] => {
                    r.push(99);
                    xs.length() * 10 + r.length()
                },
                _ => -1
            }
        }
        "#
    );
    // xs stays [1,2,3]; r becomes [2,3,99]
    assert_eq!(output.result, Ok(BexExternalValue::Int(33)));
}

#[tokio::test]
async fn pushing_to_source_does_not_mutate_rest() {
    let output = baml_test!(
        r#"
        function main() -> int {
            let xs = [1, 2, 3];
            match (xs) {
                [let a, ..let r] => {
                    xs.push(7);
                    r.length()
                },
                _ => -1
            }
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(2)));
}

#[tokio::test]
async fn rest_copy_is_shallow_elements_are_shared() {
    let output = baml_test!(
        r#"
        class Cell {
            v int
        }

        function main() -> int {
            let c = Cell { v: 1 };
            let xs = [Cell { v: 2 }, c];
            match (xs) {
                [let first, ..let r] => {
                    c.v = 5;
                    r[0].v
                },
                _ => -1
            }
        }
        "#
    );
    // r=[c]; mutating c is visible through the copied slice.
    assert_eq!(output.result, Ok(BexExternalValue::Int(5)));
}

#[tokio::test]
async fn or_arm_rest_binding_dispatches_class_branch() {
    let output = baml_test!(
        baml: OR_ARM_SOURCE,
        entry: "class_branch",
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(7)));
}

#[tokio::test]
async fn or_arm_rest_binding_dispatches_array_branch() {
    let output = baml_test!(
        baml: OR_ARM_SOURCE,
        entry: "array_branch",
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(5)));
}

#[tokio::test]
async fn or_arm_rest_binding_falls_through_empty_array() {
    let output = baml_test!(
        baml: OR_ARM_SOURCE,
        entry: "empty_branch",
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(0)));
}

const OR_ARM_SOURCE: &str = r#"
class NumberBag {
    field int[]
}

function pick(v: NumberBag | int[][]) -> int {
    match (v) {
        NumberBag { field } | [[..let field]: int[], .._] => field[0],
        _ => 0
    }
}

function class_branch() -> int {
    pick(NumberBag { field: [7, 8] })
}

function array_branch() -> int {
    pick([[5, 6], [9]])
}

function empty_branch() -> int {
    let empty: int[][] = []
    pick(empty)
}
"#;

#[tokio::test]
async fn rest_binding_in_nested_element_pattern() {
    let output = baml_test!(
        r#"
        function main() -> int {
            match ([[1, 2], [3]]) {
                [[..let inner], .._] => inner.length() * 10 + inner[1],
                _ => -1
            }
        }
        "#
    );
    // inner=[1,2]
    assert_eq!(output.result, Ok(BexExternalValue::Int(22)));
}

#[tokio::test]
async fn rest_binding_in_if_let() {
    let output = baml_test!(
        r#"
        function grab(xs: int[]) -> int {
            if let [let a, ..let r] = xs {
                return a * 10 + r.length();
            }
            return -1;
        }

        function main() -> int {
            grab([4, 5, 6]) * 100 + grab([])
        }
        "#
    );
    // grab([4,5,6]) = 42; grab([]) = -1
    assert_eq!(output.result, Ok(BexExternalValue::Int(4199)));
}

#[tokio::test]
async fn rest_only_binding_in_for_loop() {
    let output = baml_test!(
        r#"
        function main() -> int {
            let empty: int[] = [];
            let rows = [[1, 2, 3], [4], empty];
            let total = 0;
            for (let [..let r] in rows) {
                total += r.length();
            }
            total
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(4)));
}

#[tokio::test]
async fn rest_binding_generic_element_type() {
    let output = baml_test!(
        r#"
        function tail_len<T>(xs: T[]) -> int {
            match (xs) {
                [let first, ..let r] => r.length(),
                _ => -1
            }
        }

        function main() -> int {
            tail_len(["a", "b", "c"]) * 10 + tail_len([true])
        }
        "#
    );
    // 2*10 + 0
    assert_eq!(output.result, Ok(BexExternalValue::Int(20)));
}

#[tokio::test]
async fn rest_bind_chain_aliases_same_slice() {
    let output = baml_test!(
        r#"
        function main() -> int {
            match ([1, 2, 3]) {
                [..let r: let s] => r.length() * 10 + s.length()
            }
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(33)));
}
