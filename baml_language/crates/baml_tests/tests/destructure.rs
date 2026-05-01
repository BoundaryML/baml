use baml_tests::baml_test;
use bex_engine::BexExternalValue;

#[tokio::test]
async fn let_destructure_field() {
    let output = baml_test!(
        r#"
        class Animal {
            name string
        }

        function main() -> string {
            let _: Animal { name } = Animal { name: "Rex" };
            name
        }
    "#
    );

    assert_eq!(output.result, Ok(BexExternalValue::String("Rex".into())));
}

#[tokio::test]
async fn let_destructure_multiple_fields() {
    let output = baml_test!(
        r#"
        class Dog {
            name string
            breed string
        }

        function main() -> string {
            let x: Dog { name, breed } = Dog { name: "Rex", breed: "Lab" };
            name + " the " + breed
        }
    "#
    );

    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("Rex the Lab".into()))
    );
}

#[tokio::test]
async fn match_destructure_field() {
    let output = baml_test!(
        r#"
        class Animal {
            name string
        }

        function main() -> string {
            let x = Animal { name: "Buddy" };
            match (x) {
                Animal { name } => name
            }
        }
    "#
    );

    assert_eq!(output.result, Ok(BexExternalValue::String("Buddy".into())));
}

#[tokio::test]
async fn match_destructure_multi_arm() {
    let output = baml_test!(
        r#"
        class Cat {
            name string
            indoor bool
        }

        class Fish {
            name string
            freshwater bool
        }

        function main() -> string {
            let pet: Cat | Fish = Fish { name: "Nemo", freshwater: false };
            match (pet) {
                Cat { name } => name + " the cat",
                Fish { name } => name + " the fish"
            }
        }
    "#
    );

    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("Nemo the fish".into()))
    );
}

#[tokio::test]
async fn nested_destructure_two_levels() {
    let output = baml_test!(
        r#"
        class Inner {
            value string
        }

        class Outer {
            inner Inner
            label string
        }

        function main() -> string {
            let x = Outer { inner: Inner { value: "deep" }, label: "top" };
            let _: Outer { inner: Inner { value }, label } = x;
            label + ":" + value
        }
    "#
    );

    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("top:deep".into()))
    );
}

#[tokio::test]
async fn nested_destructure_three_levels() {
    let output = baml_test!(
        r#"
        class A {
            val string
        }

        class B {
            a A
        }

        class C {
            b B
        }

        function main() -> string {
            let x = C { b: B { a: A { val: "found" } } };
            let _: C { b: B { a: A { val } } } = x;
            val
        }
    "#
    );

    assert_eq!(output.result, Ok(BexExternalValue::String("found".into())));
}

#[tokio::test]
async fn or_pattern_let_destructure() {
    let output = baml_test!(
        r#"
        class Foo {
            a int
        }

        class Bar {
            a int
        }

        function main() -> int {
            let x: Foo { a } | Bar { a } = Bar { a: 42 };
            a
        }
    "#
    );

    assert_eq!(output.result, Ok(BexExternalValue::Int(42)));
}

#[tokio::test]
async fn destructure_binding_is_copy() {
    let output = baml_test!(
        r#"
        class Foo {
            a int
        }

        function main() -> int {
            let x: Foo { a } = Foo { a: 42 };
            a = 99
            x.a
        }
    "#
    );

    assert_eq!(output.result, Ok(BexExternalValue::Int(42)));
}

#[tokio::test]
async fn match_nested_destructure() {
    let output = baml_test!(
        r#"
        class Inner {
            value string
        }

        class Wrapper {
            inner Inner
            tag string
        }

        function main() -> string {
            let x = Wrapper { inner: Inner { value: "hello" }, tag: "t1" };
            match (x) {
                Wrapper { inner: Inner { value }, tag } => tag + "=" + value
            }
        }
    "#
    );

    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("t1=hello".into()))
    );
}

#[tokio::test]
async fn three_or_alternatives() {
    let output = baml_test!(
        r#"
        class A { x int }
        class B { x int }
        class C { x int }

        function main() -> int {
            let v: A | B | C = B { x: 77 };
            match (v) {
                A { x } | B { x } | C { x } => x
            }
        }
    "#
    );

    assert_eq!(output.result, Ok(BexExternalValue::Int(77)));
}

#[tokio::test]
async fn empty_destructure() {
    let output = baml_test!(
        r#"
        class Foo { a int }

        function main() -> string {
            let x = Foo { a: 1 };
            match (x) {
                Foo {} => "matched"
            }
        }
    "#
    );

    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("matched".into()))
    );
}

#[tokio::test]
async fn optional_field_destructure() {
    let output = baml_test!(
        r#"
        class MaybeNamed { name string? }

        function main() -> string {
            let x = MaybeNamed { name: "hi" };
            match (x) {
                MaybeNamed { name } => name ?? "none"
            }
        }
    "#
    );

    assert_eq!(output.result, Ok(BexExternalValue::String("hi".into())));
}

#[tokio::test]
async fn optional_field_destructure_null() {
    let output = baml_test!(
        r#"
        class MaybeNamed { name string? }

        function main() -> string {
            let x = MaybeNamed { name: null };
            match (x) {
                MaybeNamed { name } => name ?? "none"
            }
        }
    "#
    );

    assert_eq!(output.result, Ok(BexExternalValue::String("none".into())));
}

#[tokio::test]
async fn nested_or_in_field_position() {
    let output = baml_test!(
        r#"
        class Cat { name string, indoor bool }
        class Fish { name string, freshwater bool }
        class Wrapper { pet Cat | Fish }

        function main() -> string {
            let x = Wrapper { pet: Fish { name: "Nemo", freshwater: true } };
            let _: Wrapper { pet: Cat { name } | Fish { name } } = x;
            name
        }
    "#
    );

    assert_eq!(output.result, Ok(BexExternalValue::String("Nemo".into())));
}

#[tokio::test]
async fn catch_with_destructure() {
    let output = baml_test!(
        r#"
        class MyError { msg string }

        function failing() -> string {
            throw MyError { msg: "boom" }
        }

        function main() -> string {
            failing() catch (e) {
                MyError { msg } => msg,
                _ => "unknown"
            }
        }
    "#
    );

    assert_eq!(output.result, Ok(BexExternalValue::String("boom".into())));
}

// 1. Lambda capture of destructured binding
#[tokio::test]
async fn lambda_captures_destructured_binding() {
    let output = baml_test!(
        r#"
        class Foo { a int }

        function main() -> int {
            let _: Foo { a } = Foo { a: 42 };
            let f = () -> int { a }
            f()
        }
    "#
    );

    assert_eq!(output.result, Ok(BexExternalValue::Int(42)));
}

// 2. Destructure shadows outer variable
#[tokio::test]
async fn destructure_shadows_outer() {
    let output = baml_test!(
        r#"
        class Foo { a int }

        function main() -> int {
            let a = 10
            let _: Foo { a } = Foo { a: 42 };
            a
        }
    "#
    );

    assert_eq!(output.result, Ok(BexExternalValue::Int(42)));
}

// 3. Guard references destructured field
#[tokio::test]
async fn guard_sees_destructured_field() {
    let output = baml_test!(
        r#"
        class Foo { a int }

        function main() -> string {
            let x = Foo { a: 20 };
            match (x) {
                Foo { a } if a > 10 => "big",
                Foo { a } => "small"
            }
        }
    "#
    );

    assert_eq!(output.result, Ok(BexExternalValue::String("big".into())));
}

// 4. Mismatched nesting depth in or-pattern — both bind x but at different depths
#[tokio::test]
async fn or_pattern_different_nesting_depth() {
    let output = baml_test!(
        r#"
        class A { x int }
        class C { x int }
        class B { inner C }

        function main() -> int {
            let v: A | B = B { inner: C { x: 99 } };
            match (v) {
                A { x } | B { inner: C { x } } => x
            }
        }
    "#
    );

    assert_eq!(output.result, Ok(BexExternalValue::Int(99)));
}

// 5. Self-referencing init — a doesn't exist before the let
// Produces compile-time diagnostic: unresolved name: a
// Tested via snapshot in class_destructure.baml instead.

// 6. For-in destructure
// The for-in lookahead (looks_like_for_in_loop) only matches `let WORD in`,
// so `for let _: Pair { key } in items` doesn't parse as a for-in loop yet.
// This needs a parser change to support complex patterns in for-in.

// 7. Chained destructures — two class destructures in a chain
// Produces compile-time diagnostic (type mismatch or similar).
// Tested via snapshot in class_destructure.baml instead.

// 8. Destructure + throw
#[tokio::test]
async fn destructure_then_throw() {
    let output = baml_test!(
        r#"
        class Foo { a string }

        function inner() -> string {
            let _: Foo { a } = Foo { a: "thrown" };
            throw a
        }

        function main() -> string {
            inner() catch (e) {
                let s: string => s
            }
        }
    "#
    );

    assert_eq!(output.result, Ok(BexExternalValue::String("thrown".into())));
}

// 9. Binding named same as class
#[tokio::test]
async fn binding_named_same_as_class() {
    let output = baml_test!(
        r#"
        class Foo { Foo int }

        function main() -> int {
            let x = Foo { Foo: 7 };
            match (x) {
                Foo { Foo } => Foo
            }
        }
    "#
    );

    assert_eq!(output.result, Ok(BexExternalValue::Int(7)));
}

// 10. Match fallthrough — destructured binding doesn't leak to wildcard arm
#[tokio::test]
async fn destructure_doesnt_leak_to_next_arm() {
    let output = baml_test!(
        r#"
        class Foo { a int }
        class Bar { b int }

        function main() -> int {
            let x: Foo | Bar = Bar { b: 5 };
            match (x) {
                Foo { a } => a,
                _ => 0
            }
        }
    "#
    );

    assert_eq!(output.result, Ok(BexExternalValue::Int(0)));
}

// --- Adversarial edge cases ---

// For-in with destructure pattern
#[tokio::test]
async fn for_in_destructure() {
    let output = baml_test!(
        r#"
        class Pair { k string, v int }

        function main() -> int {
            let items = [Pair { k: "a", v: 1 }, Pair { k: "b", v: 2 }];
            let sum = 0;
            for let _: Pair { v } in items {
                sum += v;
            }
            sum
        }
    "#
    );

    assert_eq!(output.result, Ok(BexExternalValue::Int(3)));
}

// Destructured field used as match scrutinee
#[tokio::test]
async fn destructured_field_as_scrutinee() {
    let output = baml_test!(
        r#"
        class Wrapper { inner int | string }

        function main() -> string {
            let _: Wrapper { inner } = Wrapper { inner: 42 };
            match (inner) {
                int => "got int",
                string => "got string"
            }
        }
    "#
    );

    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("got int".into()))
    );
}

// Double-nested match: match inside match, both destructuring
#[tokio::test]
async fn nested_match_both_destructure() {
    let output = baml_test!(
        r#"
        class A { x int }
        class B { y string }
        class C { z bool }

        function main() -> string {
            let outer: A | B = A { x: 10 };
            match (outer) {
                A { x } => {
                    let inner: C | B = B { y: "hello" };
                    match (inner) {
                        B { y } => y,
                        _ => "nope"
                    }
                },
                _ => "outer miss"
            }
        }
    "#
    );

    assert_eq!(output.result, Ok(BexExternalValue::String("hello".into())));
}

// Reassigning a destructured binding
#[tokio::test]
async fn reassign_destructured_binding() {
    let output = baml_test!(
        r#"
        class Foo { a int }

        function main() -> int {
            let _: Foo { a } = Foo { a: 1 };
            a = 99;
            a
        }
    "#
    );

    assert_eq!(output.result, Ok(BexExternalValue::Int(99)));
}

// Destructure field that is itself a union, then match on it
#[tokio::test]
async fn destructure_union_field_then_match() {
    let output = baml_test!(
        r#"
        class Box { val int | string }

        function main() -> int {
            let _: Box { val } = Box { val: "hello" };
            match (val) {
                int => 0,
                string => 1
            }
        }
    "#
    );

    assert_eq!(output.result, Ok(BexExternalValue::Int(1)));
}

// Shadow: outer binding, inner match destructure with same name
#[tokio::test]
async fn shadow_outer_with_destructure() {
    let output = baml_test!(
        r#"
        class Foo { name string }

        function main() -> string {
            let name = "outer";
            let x: Foo = Foo { name: "inner" };
            match (x) {
                Foo { name } => name
            }
        }
    "#
    );

    assert_eq!(output.result, Ok(BexExternalValue::String("inner".into())));
}

// Destructure + guard using the destructured field
#[tokio::test]
async fn destructure_guard_on_nested_field() {
    let output = baml_test!(
        r#"
        class Inner { v int }
        class Outer { inner Inner }

        function main() -> int {
            let x: Outer = Outer { inner: Inner { v: 42 } };
            match (x) {
                Outer { inner: Inner { v } } if v > 100 => v,
                Outer { inner: Inner { v } } => v + 1
            }
        }
    "#
    );

    assert_eq!(output.result, Ok(BexExternalValue::Int(43)));
}

// Multiple destructures in sequence (let, then match, using fields from both)
#[tokio::test]
async fn sequential_destructures() {
    let output = baml_test!(
        r#"
        class A { x int }
        class B { y int }

        function main() -> int {
            let _: A { x } = A { x: 10 };
            let _: B { y } = B { y: 20 };
            x + y
        }
    "#
    );

    assert_eq!(output.result, Ok(BexExternalValue::Int(30)));
}

// Destructured binding passed to another function
#[tokio::test]
async fn destructured_binding_as_argument() {
    let output = baml_test!(
        r#"
        class Foo { a int }

        function double(n: int) -> int { n * 2 }

        function main() -> int {
            let _: Foo { a } = Foo { a: 21 };
            double(a)
        }
    "#
    );

    assert_eq!(output.result, Ok(BexExternalValue::Int(42)));
}

// Match with null scrutinee and destructure arm
#[tokio::test]
async fn match_null_with_destructure_arm() {
    let output = baml_test!(
        r#"
        class Foo { a int }

        function main() -> int {
            let x: Foo? = null;
            match (x) {
                Foo { a } => a,
                null => -1
            }
        }
    "#
    );

    assert_eq!(output.result, Ok(BexExternalValue::Int(-1)));
}

// For-in destructure with nested class
#[tokio::test]
async fn for_in_nested_destructure() {
    let output = baml_test!(
        r#"
        class Inner { v int }
        class Outer { inner Inner, label string }

        function main() -> int {
            let items = [
                Outer { inner: Inner { v: 10 }, label: "a" },
                Outer { inner: Inner { v: 20 }, label: "b" }
            ];
            let sum = 0;
            for let _: Outer { inner: Inner { v } } in items {
                sum += v;
            }
            sum
        }
    "#
    );

    assert_eq!(output.result, Ok(BexExternalValue::Int(30)));
}

// Destructure in both match arm AND let inside the arm body
#[tokio::test]
async fn destructure_in_arm_and_body() {
    let output = baml_test!(
        r#"
        class A { b B }
        class B { c int }

        function main() -> int {
            let x: A = A { b: B { c: 7 } };
            match (x) {
                A { b } => {
                    let _: B { c } = b;
                    c
                }
            }
        }
    "#
    );

    assert_eq!(output.result, Ok(BexExternalValue::Int(7)));
}

// Catch with destructure, then re-throw the field
#[tokio::test]
async fn catch_destructure_rethrow_field() {
    let output = baml_test!(
        r#"
        class Err1 { msg string }
        class Err2 { msg string }

        function inner() -> string {
            throw Err1 { msg: "original" }
        }

        function middle() -> string {
            inner() catch (e) {
                Err1 { msg } => throw Err2 { msg: "wrapped: " + msg },
                _ => throw Err2 { msg: "unknown" }
            }
        }

        function main() -> string {
            middle() catch (e) {
                Err2 { msg } => msg,
                _ => "nope"
            }
        }
    "#
    );

    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("wrapped: original".into()))
    );
}

// Three sequential lets destructuring same class type
#[tokio::test]
async fn three_sequential_same_class_destructure() {
    let output = baml_test!(
        r#"
        class Pt { x int, y int }

        function main() -> int {
            let a: Pt { x } = Pt { x: 1, y: 2 };
            let b: Pt { x } = Pt { x: 10, y: 20 };
            let c: Pt { x } = Pt { x: 100, y: 200 };
            x
        }
    "#
    );

    // Last destructure shadows — x should be 100
    assert_eq!(output.result, Ok(BexExternalValue::Int(100)));
}

// Or-pattern in catch
#[tokio::test]
async fn or_pattern_in_catch() {
    let output = baml_test!(
        r#"
        class E1 { msg string }
        class E2 { msg string }

        function inner() -> string {
            throw E2 { msg: "caught" }
        }

        function main() -> string {
            inner() catch (e) {
                E1 { msg } | E2 { msg } => msg,
                _ => "miss"
            }
        }
    "#
    );

    assert_eq!(output.result, Ok(BexExternalValue::String("caught".into())));
}

// Destructure a class that has many fields, only extract one
#[tokio::test]
async fn partial_destructure() {
    let output = baml_test!(
        r#"
        class Big { a int, b int, c int, d int, e int }

        function main() -> int {
            let _: Big { c } = Big { a: 1, b: 2, c: 3, d: 4, e: 5 };
            c
        }
    "#
    );

    assert_eq!(output.result, Ok(BexExternalValue::Int(3)));
}

// Destructure in if-let (not yet supported — parser doesn't handle complex patterns in if-let)
// #[tokio::test]
// async fn if_let_destructure() { ... }

// Destructure where binding name collides with a class name
#[tokio::test]
async fn binding_name_is_class_name() {
    let output = baml_test!(
        r#"
        class Foo { Foo int }

        function main() -> int {
            let _: Foo { Foo } = Foo { Foo: 7 };
            Foo
        }
    "#
    );

    assert_eq!(output.result, Ok(BexExternalValue::Int(7)));
}

// --- Deep chain + rebind tests ---

// Deep destructure with rebind at each level:
// Outer { mid: let m: Middle { inner: let i: Inner { val } } }
#[tokio::test]
async fn deep_chain_rebind_each_level() {
    let output = baml_test!(
        r#"
        class Inner { val int }
        class Middle { inner Inner, tag string }
        class Outer { mid Middle }

        function main() -> int {
            let _: Outer { mid: let m: Middle { inner: let i: Inner { val } } } =
                Outer { mid: Middle { inner: Inner { val: 99 }, tag: "t" } };
            val
        }
    "#
    );

    assert_eq!(output.result, Ok(BexExternalValue::Int(99)));
}

// Same as above but also use the intermediate rebinds
#[tokio::test]
async fn deep_chain_use_intermediate_rebinds() {
    let output = baml_test!(
        r#"
        class Inner { val int }
        class Middle { inner Inner, tag string }
        class Outer { mid Middle }

        function main() -> string {
            let _: Outer { mid: let m: Middle { inner: let i: Inner { val }, tag } } =
                Outer { mid: Middle { inner: Inner { val: 42 }, tag: "hello" } };
            tag
        }
    "#
    );

    assert_eq!(output.result, Ok(BexExternalValue::String("hello".into())));
}

// Deep chain + or-pattern: both alternatives destructure to same-named field
#[tokio::test]
async fn deep_chain_or_pattern_rebind() {
    let output = baml_test!(
        r#"
        class A { x int }
        class B { x int }
        class WrapA { inner A }
        class WrapB { inner B }

        function main() -> int {
            let v: WrapA | WrapB = WrapB { inner: B { x: 77 } };
            match (v) {
                WrapA { inner: A { x } } | WrapB { inner: B { x } } => x
            }
        }
    "#
    );

    assert_eq!(output.result, Ok(BexExternalValue::Int(77)));
}

// Three levels deep with rebind + or-pattern at the leaf
#[tokio::test]
async fn three_levels_rebind_or_at_leaf() {
    let output = baml_test!(
        r#"
        class Leaf1 { val string }
        class Leaf2 { val string }
        class Mid { leaf Leaf1 | Leaf2 }
        class Top { mid Mid }

        function main() -> string {
            let t = Top { mid: Mid { leaf: Leaf2 { val: "found" } } };
            match (t) {
                Top { mid: Mid { leaf: Leaf1 { val } | Leaf2 { val } } } => val
            }
        }
    "#
    );

    assert_eq!(output.result, Ok(BexExternalValue::String("found".into())));
}

// Rebind in field position, then use the rebound name (not the field name)
#[tokio::test]
async fn field_rebind_use_new_name() {
    let output = baml_test!(
        r#"
        class Pair { first int, second int }

        function main() -> int {
            let _: Pair { first: let a, second: let b } = Pair { first: 3, second: 7 };
            a + b
        }
    "#
    );

    assert_eq!(output.result, Ok(BexExternalValue::Int(10)));
}

// Rebind with type annotation in field position
#[tokio::test]
async fn field_rebind_with_type() {
    let output = baml_test!(
        r#"
        class Box { val int }

        function main() -> int {
            let _: Box { val: let v: int } = Box { val: 55 };
            v
        }
    "#
    );

    assert_eq!(output.result, Ok(BexExternalValue::Int(55)));
}

// Match arm: deep chain with rebind + guard on rebound variable
#[tokio::test]
async fn match_deep_rebind_guard() {
    let output = baml_test!(
        r#"
        class Inner { n int }
        class Outer { inner Inner }

        function main() -> int {
            let x = Outer { inner: Inner { n: 5 } };
            match (x) {
                Outer { inner: let i: Inner { n } } if n > 10 => n * 100,
                Outer { inner: let i: Inner { n } } => n
            }
        }
    "#
    );

    assert_eq!(output.result, Ok(BexExternalValue::Int(5)));
}

// For-in with deep destructure + rebind
#[tokio::test]
async fn for_in_deep_rebind() {
    let output = baml_test!(
        r#"
        class Inner { v int }
        class Outer { inner Inner }

        function main() -> int {
            let items = [
                Outer { inner: Inner { v: 10 } },
                Outer { inner: Inner { v: 20 } }
            ];
            let sum = 0;
            for let _: Outer { inner: let i: Inner { v } } in items {
                sum += v;
            }
            sum
        }
    "#
    );

    assert_eq!(output.result, Ok(BexExternalValue::Int(30)));
}

// Or-pattern where both alternatives are the SAME type with deep rebind
#[tokio::test]
async fn or_same_type_deep_rebind() {
    let output = baml_test!(
        r#"
        class Inner { val int }
        class Wrap { inner Inner, kind string }

        function main() -> int {
            let x: Wrap = Wrap { inner: Inner { val: 42 }, kind: "second" };
            match (x) {
                Wrap { inner: Inner { val }, kind } if kind == "first" => val * 10,
                Wrap { inner: Inner { val }, kind } if kind == "second" => val
            }
        }
    "#
    );

    assert_eq!(output.result, Ok(BexExternalValue::Int(42)));
}

// Three or-alternatives, each with deep destructure + rebind
#[tokio::test]
async fn three_or_alternatives_deep_rebind() {
    let output = baml_test!(
        r#"
        class X { n int }
        class Y { n int }
        class Z { n int }
        class WX { inner X }
        class WY { inner Y }
        class WZ { inner Z }

        function main() -> int {
            let v: WX | WY | WZ = WZ { inner: Z { n: 33 } };
            match (v) {
                WX { inner: X { n } } | WY { inner: Y { n } } | WZ { inner: Z { n } } => n
            }
        }
    "#
    );

    assert_eq!(output.result, Ok(BexExternalValue::Int(33)));
}

// Four alternatives: same wrapper type but different inner types
#[tokio::test]
async fn four_alternatives_mixed() {
    let output = baml_test!(
        r#"
        class A { val int }
        class B { val int }
        class C { val int }
        class D { val int }

        function main() -> int {
            let v: A | B | C | D = C { val: 100 };
            match (v) {
                A { val } | B { val } | C { val } | D { val } => val
            }
        }
    "#
    );

    assert_eq!(output.result, Ok(BexExternalValue::Int(100)));
}

// Same type both sides of or, with different guards picking different arms
#[tokio::test]
async fn or_same_type_both_sides_guard_dispatch() {
    let output = baml_test!(
        r#"
        class Pt { x int, y int }

        function main() -> int {
            let p = Pt { x: 3, y: 7 };
            match (p) {
                Pt { x, y } if x > y => x,
                Pt { x, y } => y
            }
        }
    "#
    );

    assert_eq!(output.result, Ok(BexExternalValue::Int(7)));
}

// --- Torture tests ---

// Or-pattern in a for-in loop
#[tokio::test]
async fn for_in_or_pattern() {
    let output = baml_test!(
        r#"
        class A { val int }
        class B { val int }

        function main() -> int {
            let items: (A | B)[] = [A { val: 1 }, B { val: 2 }, A { val: 3 }];
            let sum = 0;
            for let _: A { val } | B { val } in items {
                sum += val;
            }
            sum
        }
    "#
    );

    assert_eq!(output.result, Ok(BexExternalValue::Int(6)));
}

// Nested or inside nested or:
// (A { inner: C { x } | D { x } }) | (B { inner: E { x } | F { x } })
#[tokio::test]
async fn nested_or_inside_or() {
    let output = baml_test!(
        r#"
        class C { x int }
        class D { x int }
        class E { x int }
        class F { x int }
        class A { inner C | D }
        class B { inner E | F }

        function main() -> int {
            let v: A | B = B { inner: F { x: 42 } };
            match (v) {
                A { inner: C { x } | D { x } } | B { inner: E { x } | F { x } } => x
            }
        }
    "#
    );

    assert_eq!(output.result, Ok(BexExternalValue::Int(42)));
}

// Guard referencing multiple destructured fields
#[tokio::test]
async fn guard_multiple_fields() {
    let output = baml_test!(
        r#"
        class Rect { w int, h int }

        function main() -> string {
            let r = Rect { w: 5, h: 5 };
            match (r) {
                Rect { w, h } if w == h => "square",
                Rect { w, h } if w > h => "wide",
                Rect { w, h } => "tall"
            }
        }
    "#
    );

    assert_eq!(output.result, Ok(BexExternalValue::String("square".into())));
}

// Destructure + break from for-in
#[tokio::test]
async fn for_in_destructure_break() {
    let output = baml_test!(
        r#"
        class Item { val int }

        function main() -> int {
            let items = [Item { val: 1 }, Item { val: 99 }, Item { val: 3 }];
            let result = 0;
            for let _: Item { val } in items {
                if val > 50 {
                    result = val;
                    break;
                }
            }
            result
        }
    "#
    );

    assert_eq!(output.result, Ok(BexExternalValue::Int(99)));
}

// Destructure + continue in for-in
#[tokio::test]
async fn for_in_destructure_continue() {
    let output = baml_test!(
        r#"
        class Item { val int }

        function main() -> int {
            let items = [Item { val: 1 }, Item { val: 2 }, Item { val: 3 }, Item { val: 4 }];
            let sum = 0;
            for let _: Item { val } in items {
                if val % 2 == 0 {
                    continue;
                }
                sum += val;
            }
            sum
        }
    "#
    );

    // 1 + 3 = 4 (skipped 2 and 4)
    assert_eq!(output.result, Ok(BexExternalValue::Int(4)));
}

// Catch with or-pattern where alternatives have different nesting depths
#[tokio::test]
async fn catch_or_different_nesting_depth() {
    let output = baml_test!(
        r#"
        class SimpleErr { msg string }
        class Wrapper { inner SimpleErr }

        function failing() -> string {
            throw Wrapper { inner: SimpleErr { msg: "deep" } }
        }

        function main() -> string {
            failing() catch (e) {
                SimpleErr { msg } | Wrapper { inner: SimpleErr { msg } } => msg,
                _ => "unknown"
            }
        }
    "#
    );

    assert_eq!(output.result, Ok(BexExternalValue::String("deep".into())));
}

// Empty destructure in or-pattern — both sides match class but extract nothing
#[tokio::test]
async fn or_empty_destructures() {
    let output = baml_test!(
        r#"
        class A { x int }
        class B { y int }

        function main() -> string {
            let v: A | B = B { y: 1 };
            match (v) {
                A {} | B {} => "matched"
            }
        }
    "#
    );

    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("matched".into()))
    );
}

// Destructure where a field is a list, then iterate it
#[tokio::test]
async fn destructure_list_field_then_iterate() {
    let output = baml_test!(
        r#"
        class Bag { items int[] }

        function main() -> int {
            let _: Bag { items } = Bag { items: [10, 20, 30] };
            let sum = 0;
            for let x in items {
                sum += x;
            }
            sum
        }
    "#
    );

    assert_eq!(output.result, Ok(BexExternalValue::Int(60)));
}

// Reassign variable, then re-destructure
#[tokio::test]
async fn reassign_then_re_destructure() {
    let output = baml_test!(
        r#"
        class Pt { x int, y int }

        function main() -> int {
            let p = Pt { x: 1, y: 2 };
            let _: Pt { x } = p;
            let first = x;
            p = Pt { x: 10, y: 20 };
            let _: Pt { x } = p;
            first + x
        }
    "#
    );

    // 1 + 10 = 11
    assert_eq!(output.result, Ok(BexExternalValue::Int(11)));
}

// Match arm where one or-alt is a wildcard — should always match
#[tokio::test]
async fn or_with_wildcard() {
    let output = baml_test!(
        r#"
        class Foo { a int }

        function main() -> int {
            let x: Foo | int = 42;
            match (x) {
                Foo {} | _ => 1
            }
        }
    "#
    );

    assert_eq!(output.result, Ok(BexExternalValue::Int(1)));
}

// Deeply nested for-in with or-pattern and break
#[tokio::test]
async fn for_in_nested_or_break() {
    let output = baml_test!(
        r#"
        class Ok { val int }
        class Err { val int }

        function main() -> int {
            let items: (Ok | Err)[] = [Ok { val: 1 }, Err { val: 2 }, Ok { val: 100 }];
            let found = -1;
            for let _: Ok { val } | Err { val } in items {
                if val > 50 {
                    found = val;
                    break;
                }
            }
            found
        }
    "#
    );

    assert_eq!(output.result, Ok(BexExternalValue::Int(100)));
}

// Destructure in match, use field to index into an array
#[tokio::test]
async fn destructure_field_as_index() {
    let output = baml_test!(
        r#"
        class Selector { idx int }

        function main() -> string {
            let arr = ["zero", "one", "two", "three"];
            let s = Selector { idx: 2 };
            match (s) {
                Selector { idx } => arr[idx]
            }
        }
    "#
    );

    assert_eq!(output.result, Ok(BexExternalValue::String("two".into())));
}

// Triple nesting: destructure deep, capture in lambda
#[tokio::test]
async fn deep_destructure_into_lambda() {
    let output = baml_test!(
        r#"
        class Inner { val int }
        class Mid { inner Inner }
        class Outer { mid Mid }

        function apply(f: () -> int) -> int { f() }

        function main() -> int {
            let o = Outer { mid: Mid { inner: Inner { val: 7 } } };
            let _: Outer { mid: Mid { inner: Inner { val } } } = o;
            apply(() -> int { val * 6 })
        }
    "#
    );

    assert_eq!(output.result, Ok(BexExternalValue::Int(42)));
}

// Catch chain: destructure error, throw new error with field, catch again
#[tokio::test]
async fn catch_chain_destructure() {
    let output = baml_test!(
        r#"
        class E1 { code int }
        class E2 { code int, extra string }

        function step1() -> int {
            throw E1 { code: 404 }
        }

        function step2() -> int {
            step1() catch (e) {
                E1 { code } => throw E2 { code: code, extra: "enriched" },
                _ => 0
            }
        }

        function main() -> int {
            step2() catch (e) {
                E2 { code, extra } => code,
                _ => -1
            }
        }
    "#
    );

    assert_eq!(output.result, Ok(BexExternalValue::Int(404)));
}

// --- Integration tests: exercise every alternative ---

const OR_TWO_CLASSES: &str = r#"
    class Cat { name string, indoor bool }
    class Fish { name string, freshwater bool }

    function classify(pet: Cat | Fish) -> string {
        match (pet) {
            Cat { name, indoor } => name + if indoor { " (indoor)" } else { " (outdoor)" },
            Fish { name, freshwater } => name + if freshwater { " (fresh)" } else { " (salt)" }
        }
    }
"#;

#[tokio::test]
async fn integ_or_two_classes_cat() {
    let output = baml_test!(
        baml: OR_TWO_CLASSES,
        entry: "classify",
        args: {
            "pet" => BexExternalValue::Instance {
                class_name: "Cat".into(),
                fields: vec![
                    ("name".into(), BexExternalValue::String("Luna".into())),
                    ("indoor".into(), BexExternalValue::Bool(true)),
                ].into_iter().collect(),
            }
        },
    );
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("Luna (indoor)".into()))
    );
}

#[tokio::test]
async fn integ_or_two_classes_fish() {
    let output = baml_test!(
        baml: OR_TWO_CLASSES,
        entry: "classify",
        args: {
            "pet" => BexExternalValue::Instance {
                class_name: "Fish".into(),
                fields: vec![
                    ("name".into(), BexExternalValue::String("Nemo".into())),
                    ("freshwater".into(), BexExternalValue::Bool(false)),
                ].into_iter().collect(),
            }
        },
    );
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("Nemo (salt)".into()))
    );
}

const OR_DIFFERENT_DEPTH: &str = r#"
    class Leaf { val int }
    class Nested { inner Leaf }

    function extract(x: Leaf | Nested) -> int {
        match (x) {
            Leaf { val } | Nested { inner: Leaf { val } } => val
        }
    }
"#;

#[tokio::test]
async fn integ_different_depth_shallow() {
    let output = baml_test!(
        baml: OR_DIFFERENT_DEPTH,
        entry: "extract",
        args: {
            "x" => BexExternalValue::Instance {
                class_name: "Leaf".into(),
                fields: vec![("val".into(), BexExternalValue::Int(11))].into_iter().collect(),
            }
        },
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(11)));
}

#[tokio::test]
async fn integ_different_depth_deep() {
    let output = baml_test!(
        baml: OR_DIFFERENT_DEPTH,
        entry: "extract",
        args: {
            "x" => BexExternalValue::Instance {
                class_name: "Nested".into(),
                fields: vec![
                    ("inner".into(), BexExternalValue::Instance {
                        class_name: "Leaf".into(),
                        fields: vec![("val".into(), BexExternalValue::Int(99))].into_iter().collect(),
                    }),
                ].into_iter().collect(),
            }
        },
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(99)));
}

const OR_THREE_CLASSES: &str = r#"
    class Red { r int }
    class Green { g int }
    class Blue { b int }

    function channel(c: Red | Green | Blue) -> int {
        match (c) {
            Red { r } | Green { g: let r } | Blue { b: let r } => r
        }
    }
"#;

#[tokio::test]
async fn integ_three_classes_first() {
    let output = baml_test!(
        baml: OR_THREE_CLASSES,
        entry: "channel",
        args: {
            "c" => BexExternalValue::Instance {
                class_name: "Red".into(),
                fields: vec![("r".into(), BexExternalValue::Int(255))].into_iter().collect(),
            }
        },
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(255)));
}

#[tokio::test]
async fn integ_three_classes_second() {
    let output = baml_test!(
        baml: OR_THREE_CLASSES,
        entry: "channel",
        args: {
            "c" => BexExternalValue::Instance {
                class_name: "Green".into(),
                fields: vec![("g".into(), BexExternalValue::Int(128))].into_iter().collect(),
            }
        },
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(128)));
}

#[tokio::test]
async fn integ_three_classes_third() {
    let output = baml_test!(
        baml: OR_THREE_CLASSES,
        entry: "channel",
        args: {
            "c" => BexExternalValue::Instance {
                class_name: "Blue".into(),
                fields: vec![("b".into(), BexExternalValue::Int(64))].into_iter().collect(),
            }
        },
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(64)));
}

const OR_NESTED_OR: &str = r#"
    class C { x int }
    class D { x int }
    class E { x int }
    class F { x int }
    class A { inner C | D }
    class B { inner E | F }

    function dig(v: A | B) -> int {
        match (v) {
            A { inner: C { x } | D { x } } | B { inner: E { x } | F { x } } => x
        }
    }
"#;

#[tokio::test]
async fn integ_nested_or_a_c() {
    let output = baml_test!(
        baml: OR_NESTED_OR,
        entry: "dig",
        args: {
            "v" => BexExternalValue::Instance {
                class_name: "A".into(),
                fields: vec![
                    ("inner".into(), BexExternalValue::Instance {
                        class_name: "C".into(),
                        fields: vec![("x".into(), BexExternalValue::Int(1))].into_iter().collect(),
                    }),
                ].into_iter().collect(),
            }
        },
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(1)));
}

#[tokio::test]
async fn integ_nested_or_a_d() {
    let output = baml_test!(
        baml: OR_NESTED_OR,
        entry: "dig",
        args: {
            "v" => BexExternalValue::Instance {
                class_name: "A".into(),
                fields: vec![
                    ("inner".into(), BexExternalValue::Instance {
                        class_name: "D".into(),
                        fields: vec![("x".into(), BexExternalValue::Int(2))].into_iter().collect(),
                    }),
                ].into_iter().collect(),
            }
        },
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(2)));
}

#[tokio::test]
async fn integ_nested_or_b_e() {
    let output = baml_test!(
        baml: OR_NESTED_OR,
        entry: "dig",
        args: {
            "v" => BexExternalValue::Instance {
                class_name: "B".into(),
                fields: vec![
                    ("inner".into(), BexExternalValue::Instance {
                        class_name: "E".into(),
                        fields: vec![("x".into(), BexExternalValue::Int(3))].into_iter().collect(),
                    }),
                ].into_iter().collect(),
            }
        },
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(3)));
}

#[tokio::test]
async fn integ_nested_or_b_f() {
    let output = baml_test!(
        baml: OR_NESTED_OR,
        entry: "dig",
        args: {
            "v" => BexExternalValue::Instance {
                class_name: "B".into(),
                fields: vec![
                    ("inner".into(), BexExternalValue::Instance {
                        class_name: "F".into(),
                        fields: vec![("x".into(), BexExternalValue::Int(4))].into_iter().collect(),
                    }),
                ].into_iter().collect(),
            }
        },
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(4)));
}

const GUARD_DISPATCH: &str = r#"
    class A { x int }
    class B { x int }

    function route(v: A | B) -> int {
        match (v) {
            A { x } | B { x } if x > 100 => -1,
            A { x } => x,
            B { x } => x + 1000
        }
    }
"#;

#[tokio::test]
async fn integ_guard_dispatch_a_big() {
    let output = baml_test!(
        baml: GUARD_DISPATCH,
        entry: "route",
        args: {
            "v" => BexExternalValue::Instance {
                class_name: "A".into(),
                fields: vec![("x".into(), BexExternalValue::Int(200))].into_iter().collect(),
            }
        },
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(-1)));
}

#[tokio::test]
async fn integ_guard_dispatch_b_big() {
    let output = baml_test!(
        baml: GUARD_DISPATCH,
        entry: "route",
        args: {
            "v" => BexExternalValue::Instance {
                class_name: "B".into(),
                fields: vec![("x".into(), BexExternalValue::Int(999))].into_iter().collect(),
            }
        },
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(-1)));
}

#[tokio::test]
async fn integ_guard_dispatch_a_small() {
    let output = baml_test!(
        baml: GUARD_DISPATCH,
        entry: "route",
        args: {
            "v" => BexExternalValue::Instance {
                class_name: "A".into(),
                fields: vec![("x".into(), BexExternalValue::Int(5))].into_iter().collect(),
            }
        },
    );
    // A { x } => x (returns x directly)
    assert_eq!(output.result, Ok(BexExternalValue::Int(5)));
}

#[tokio::test]
async fn integ_guard_dispatch_b_small() {
    let output = baml_test!(
        baml: GUARD_DISPATCH,
        entry: "route",
        args: {
            "v" => BexExternalValue::Instance {
                class_name: "B".into(),
                fields: vec![("x".into(), BexExternalValue::Int(3))].into_iter().collect(),
            }
        },
    );
    // B { x } => x + 1000
    assert_eq!(output.result, Ok(BexExternalValue::Int(1003)));
}

#[tokio::test]
async fn two_let_bind_chain() {
    let output = baml_test!(
        r#"
        function main() -> int {
            let x: let y = 1;
            x + y
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(2)));
}

#[tokio::test]
async fn multi_let_bind_chain() {
    let output = baml_test!(
        r#"
        function main() -> int {
            let x: let y: let z = 1;
            x + y + z
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(3)));
}

#[tokio::test]
async fn bind_chain_with_destructure() {
    let output = baml_test!(
        r#"
        class Wrapper {
            value int
        }

        function main() -> int {
            let x: Wrapper { value }: let y = Wrapper { value: 42 };
            value + y.value + x.value
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(126)));
}

#[tokio::test]
async fn bind_chain_types_are_correct() {
    let output = baml_test!(
        r#"
        class Wrapper {
            value int
        }

        function get_value(w: Wrapper) -> int {
            w.value
        }

        function main() -> int {
            let x: Wrapper { value }: let y = Wrapper { value: 10 };
            // x should be Wrapper, value should be int, y should be Wrapper
            get_value(x) + value + get_value(y)
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(30)));
}

// ── Let torture tests ────────────────────────────────────────────────────────

#[tokio::test]
async fn let_deep_chain_with_nested_destructure() {
    let output = baml_test!(
        r#"
        class Inner {
            val int
        }

        class Outer {
            inner Inner
            tag string
        }

        function main() -> int {
            let whole: Outer { inner: Inner { val }, tag }: let alias = Outer {
                inner: Inner { val: 7 },
                tag: "hello"
            };
            // whole=Outer, val=7, tag="hello", alias=Outer
            val + whole.inner.val + alias.inner.val + tag.length()
        }
    "#
    );
    // 7 + 7 + 7 + 5 = 26
    assert_eq!(output.result, Ok(BexExternalValue::Int(26)));
}

#[tokio::test]
async fn let_three_bind_chain_with_destructure_in_middle() {
    let output = baml_test!(
        r#"
        class Pair {
            a int
            b int
        }

        function main() -> int {
            let x: Pair { a, b }: let y: let z = Pair { a: 3, b: 4 };
            a + b + x.a + y.b + z.a
        }
    "#
    );
    // 3 + 4 + 3 + 4 + 3 = 17
    assert_eq!(output.result, Ok(BexExternalValue::Int(17)));
}

// ── Match torture tests ──────────────────────────────────────────────────────

#[tokio::test]
async fn match_arm_bind_with_deep_destructure() {
    let output = baml_test!(
        r#"
        class Inner {
            val int
        }

        class Outer {
            inner Inner
            tag string
        }

        function main() -> int {
            let o = Outer { inner: Inner { val: 99 }, tag: "found" };
            match (o) {
                let w: Outer { inner: Inner { val }, tag } => {
                    val + w.inner.val + tag.length()
                }
            }
        }
    "#
    );
    // 99 + 99 + 5 = 203
    assert_eq!(output.result, Ok(BexExternalValue::Int(203)));
}

#[tokio::test]
async fn match_union_arms_with_destructure_chains() {
    let output = baml_test!(
        r#"
        class Dog {
            type "dog"
            name string
            tricks int
        }

        class Cat {
            type "cat"
            name string
            lives int
        }

        type Pet = Cat | Dog

        function main() -> int {
            let pet: Pet = Cat { type: "cat", name: "Whiskers", lives: 9 };
            match (pet) {
                let whole: Cat { name, lives } => {
                    lives + name.length() + whole.lives
                },
                let whole: Dog { name, tricks } => {
                    tricks + name.length() + whole.tricks
                }
            }
        }
    "#
    );
    // 9 + 8 + 9 = 26
    assert_eq!(output.result, Ok(BexExternalValue::Int(26)));
}

#[tokio::test]
async fn match_chain_bind_after_type_narrowing() {
    let output = baml_test!(
        r#"
        class Dog {
            type "dog"
            name string
        }

        class Cat {
            type "cat"
            name string
        }

        type Pet = Cat | Dog

        function main() -> string {
            let pet: Pet = Dog { type: "dog", name: "Buddy" };
            match (pet) {
                let d: Dog { name } => {
                    name + "=" + d.name
                },
                let c: Cat { name } => name
            }
        }
    "#
    );
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("Buddy=Buddy".to_string()))
    );
}

// ── Wildcard chain tests ─────────────────────────────────────────────────────

#[tokio::test]
async fn wildcard_in_chain() {
    let output = baml_test!(
        r#"
        function main() -> int {
            let x: _: let y = 1;
            x + y
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(2)));
}

#[tokio::test]
async fn wildcard_between_destructures() {
    let output = baml_test!(
        r#"
        class Pair {
            a int
            b int
        }

        function main() -> int {
            let x: _: Pair { a, b }: let y = Pair { a: 3, b: 4 };
            a + b + x.a + y.b
        }
    "#
    );
    // 3 + 4 + 3 + 4 = 14
    assert_eq!(output.result, Ok(BexExternalValue::Int(14)));
}

#[tokio::test]
async fn wildcard_in_match_arm() {
    let output = baml_test!(
        r#"
        class Dog {
            type "dog"
            name string
        }

        class Cat {
            type "cat"
            name string
        }

        type Pet = Cat | Dog

        function main() -> string {
            let pet: Pet = Cat { type: "cat", name: "Milo" };
            match (pet) {
                let c: _: Cat { name } => name,
                _ => "other"
            }
        }
    "#
    );
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("Milo".to_string()))
    );
}

// ── Or-pattern torture (4+ alternatives, deep nesting) ───────────────────────

#[tokio::test]
async fn or_four_alternatives_with_rebind() {
    let output = baml_test!(
        r#"
        class A { type "a" val int }
        class B { type "b" val int }
        class C { type "c" val int }
        class D { type "d" val int }

        type ABCD = A | B | C | D

        function main() -> int {
            let items: ABCD[] = [
                A { type: "a", val: 1 },
                B { type: "b", val: 2 },
                C { type: "c", val: 3 },
                D { type: "d", val: 4 },
            ];
            let sum = 0;
            for (let item in items) {
                sum = sum + match (item) {
                    let x: A { val } => val * 10,
                    let x: B { val } => val * 100,
                    let x: C { val } => val * 1000,
                    let x: D { val } => val * 10000
                };
            }
            sum
        }
    "#
    );
    // 1*10 + 2*100 + 3*1000 + 4*10000 = 10 + 200 + 3000 + 40000 = 43210
    assert_eq!(output.result, Ok(BexExternalValue::Int(43210)));
}

#[tokio::test]
async fn or_five_alternatives_deep_destructure() {
    let output = baml_test!(
        r#"
        class Inner { n int }
        class V1 { type "v1" inner Inner }
        class V2 { type "v2" inner Inner }
        class V3 { type "v3" inner Inner }
        class V4 { type "v4" inner Inner }
        class V5 { type "v5" inner Inner }

        type Variant = V1 | V2 | V3 | V4 | V5

        function main() -> int {
            let items: Variant[] = [
                V1 { type: "v1", inner: Inner { n: 1 } },
                V2 { type: "v2", inner: Inner { n: 2 } },
                V3 { type: "v3", inner: Inner { n: 3 } },
                V4 { type: "v4", inner: Inner { n: 4 } },
                V5 { type: "v5", inner: Inner { n: 5 } },
            ];
            let sum = 0;
            for (let item in items) {
                sum = sum + match (item) {
                    let w: V1 { inner: Inner { n } } => n,
                    let w: V2 { inner: Inner { n } } => n + w.inner.n,
                    let w: V3 { inner: Inner { n } } => n * n,
                    let w: V4 { inner: Inner { n } } => n * 100,
                    let w: V5 { inner: Inner { n } } => n * 1000,
                };
            }
            sum
        }
    "#
    );
    // V1: 1, V2: 2+2=4, V3: 3*3=9, V4: 4*100=400, V5: 5*1000=5000
    // 1 + 4 + 9 + 400 + 5000 = 5414
    assert_eq!(output.result, Ok(BexExternalValue::Int(5414)));
}

#[tokio::test]
async fn or_pattern_four_alternatives_let() {
    let output = baml_test!(
        r#"
        class A { type "a" val int }
        class B { type "b" val int }
        class C { type "c" val int }
        class D { type "d" val int }

        type ABCD = A | B | C | D

        function main() -> int {
            let x: ABCD = C { type: "c", val: 77 };
            match (x) {
                let z: A { val } | B { val } | C { val } | D { val } => val
            }
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(77)));
}

#[tokio::test]
async fn or_pattern_top_level_split() {
    let output = baml_test!(
        r#"
        class A { type "a" val int }
        class B { type "b" val int }
        class C { type "c" val int }
        class D { type "d" val int }

        type ABCD = A | B | C | D

        function main() -> int {
            let x: ABCD = D { type: "d", val: 99 };
            match (x) {
                let z: A { val } | let z: B { val } | let z: C { val } | let z: D { val } => val
            }
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(99)));
}

#[tokio::test]
async fn top_level_split_chain_binds_both_sides() {
    let output = baml_test!(
        r#"
        class A { type "a" val int }
        class B { type "b" val int }

        type AB = A | B

        function main() -> int {
            let x: AB = B { type: "b", val: 7 };
            match (x) {
                let w: let x: A { val } | let w: let x: B { val } => val + 1
            }
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(8)));
}

#[tokio::test]
async fn top_level_split_three_way() {
    let output = baml_test!(
        r#"
        class A { type "a" val int }
        class B { type "b" val int }
        class C { type "c" val int }

        type ABC = A | B | C

        function main() -> int {
            let x: ABC = C { type: "c", val: 33 };
            match (x) {
                let z: A { val } | let z: B { val } | let z: C { val } => val
            }
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(33)));
}

#[tokio::test]
async fn top_level_split_with_wildcard_chains() {
    let output = baml_test!(
        r#"
        class A { type "a" val int }
        class B { type "b" val int }

        type AB = A | B

        function main() -> int {
            let x: AB = A { type: "a", val: 5 };
            match (x) {
                let x: _: A { val } | let x: _: B { val } => val
            }
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(5)));
}

#[tokio::test]
async fn top_level_split_deep_destructure() {
    let output = baml_test!(
        r#"
        class Inner { val int }
        class Outer { type "outer" inner Inner tag string }
        class Other { type "other" inner Inner tag string }

        type OO = Outer | Other

        function main() -> int {
            let x: OO = Other { type: "other", inner: Inner { val: 42 }, tag: "hello" };
            match (x) {
                let z: Outer { inner: Inner { val } } | let z: Other { inner: Inner { val } } => val
            }
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(42)));
}

#[tokio::test]
async fn mixed_arms_chain_local_and_top_level_split() {
    let output = baml_test!(
        r#"
        class A { type "a" val int }
        class B { type "b" val int }
        class C { type "c" val int }

        type ABC = A | B | C

        function main() -> int {
            let x: ABC = C { type: "c", val: 10 };
            match (x) {
                let z: A { val } | B { val } => val,
                let z: C { val } | let z: C { val } => val + 100
            }
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(110)));
}

// --- for-in torture tests ---

#[tokio::test]
async fn for_in_chain_bind() {
    let output = baml_test!(
        r#"
        class Item { val int }

        function main() -> int {
            let items = [Item { val: 10 }, Item { val: 20 }];
            let sum = 0;
            for let x: let y: Item { val } in items {
                sum += val;
            }
            sum
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(30)));
}

#[tokio::test]
async fn for_in_wildcard_chain() {
    let output = baml_test!(
        r#"
        class Item { val int }

        function main() -> int {
            let items = [Item { val: 5 }, Item { val: 7 }];
            let sum = 0;
            for let x: _: Item { val } in items {
                sum += val;
            }
            sum
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(12)));
}

#[tokio::test]
async fn for_in_deep_destructure_chain() {
    let output = baml_test!(
        r#"
        class Inner { val int }
        class Outer { inner Inner, tag string }

        function main() -> int {
            let items = [
                Outer { inner: Inner { val: 1 }, tag: "a" },
                Outer { inner: Inner { val: 2 }, tag: "bb" },
                Outer { inner: Inner { val: 3 }, tag: "ccc" }
            ];
            let total = 0;
            for let whole: Outer { inner: Inner { val }, tag } in items {
                total += val + tag.length();
            }
            total
        }
    "#
    );
    // (1+1) + (2+2) + (3+3) = 12
    assert_eq!(output.result, Ok(BexExternalValue::Int(12)));
}

#[tokio::test]
async fn for_in_or_pattern_four_alternatives() {
    let output = baml_test!(
        r#"
        class A { type "a" val int }
        class B { type "b" val int }
        class C { type "c" val int }
        class D { type "d" val int }

        function main() -> int {
            let items: (A | B | C | D)[] = [
                A { type: "a", val: 1 },
                B { type: "b", val: 2 },
                C { type: "c", val: 4 },
                D { type: "d", val: 8 }
            ];
            let sum = 0;
            for let _: A { val } | B { val } | C { val } | D { val } in items {
                sum += val;
            }
            sum
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(15)));
}

#[tokio::test]
async fn for_in_top_level_split_or() {
    let output = baml_test!(
        r#"
        class A { type "a" val int }
        class B { type "b" val int }

        function main() -> int {
            let items: (A | B)[] = [A { type: "a", val: 3 }, B { type: "b", val: 7 }];
            let sum = 0;
            for let z: A { val } | let z: B { val } in items {
                sum += val;
            }
            sum
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(10)));
}

#[tokio::test]
async fn for_in_nested_or_deep_destructure() {
    let output = baml_test!(
        r#"
        class Inner { val int }
        class A { type "a" inner Inner }
        class B { type "b" inner Inner }

        function main() -> int {
            let items: (A | B)[] = [
                A { type: "a", inner: Inner { val: 10 } },
                B { type: "b", inner: Inner { val: 20 } },
                A { type: "a", inner: Inner { val: 30 } }
            ];
            let sum = 0;
            for let _: A { inner: Inner { val } } | B { inner: Inner { val } } in items {
                sum += val;
            }
            sum
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(60)));
}

#[tokio::test]
async fn for_in_chain_bind_with_break_continue() {
    let output = baml_test!(
        r#"
        class Item { val int }

        function main() -> int {
            let items = [Item { val: 1 }, Item { val: 2 }, Item { val: 100 }, Item { val: 3 }];
            let sum = 0;
            for let whole: Item { val } in items {
                if val < 0 {
                    continue;
                }
                if val > 50 {
                    break;
                }
                sum += val;
            }
            sum
        }
    "#
    );
    // 1 + 2 = 3, then break at 100
    assert_eq!(output.result, Ok(BexExternalValue::Int(3)));
}

#[tokio::test]
async fn for_in_nested_loops_destructure() {
    let output = baml_test!(
        r#"
        class Row { cells int[] }

        function main() -> int {
            let rows = [
                Row { cells: [1, 2] },
                Row { cells: [3, 4] }
            ];
            let sum = 0;
            for let _: Row { cells } in rows {
                for let c in cells {
                    sum += c;
                }
            }
            sum
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(10)));
}

#[tokio::test]
async fn for_in_triple_chain_bind() {
    let output = baml_test!(
        r#"
        class Item { val int }

        function main() -> int {
            let items = [Item { val: 5 }, Item { val: 15 }];
            let sum = 0;
            for let a: let b: let c: Item { val } in items {
                sum += val;
            }
            sum
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(20)));
}

#[tokio::test]
async fn for_in_or_with_match_inside() {
    let output = baml_test!(
        r#"
        class A { type "a" val int }
        class B { type "b" val int }

        function main() -> int {
            let items: (A | B)[] = [
                A { type: "a", val: 1 },
                B { type: "b", val: 10 },
                A { type: "a", val: 100 }
            ];
            let sum = 0;
            for let item: A { val } | B { val } in items {
                let mult = match (item) {
                    A {} => 1,
                    B {} => 2
                };
                sum += val * mult;
            }
            sum
        }
    "#
    );
    // A:1 + B:10*2 + A:100 = 121
    assert_eq!(output.result, Ok(BexExternalValue::Int(121)));
}

#[tokio::test]
async fn for_in_wildcard_between_bind_and_destructure() {
    let output = baml_test!(
        r#"
        class Pair { a int, b int }

        function main() -> int {
            let items = [Pair { a: 1, b: 2 }, Pair { a: 3, b: 4 }];
            let sum = 0;
            for let x: _: Pair { a, b }: let y in items {
                sum += a + b;
            }
            sum
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(10)));
}

#[tokio::test]
async fn for_in_nested_or_destructure_torture() {
    let output = baml_test!(
        r#"
        class Inner { val int }
        class A { type "a" inner Inner }
        class B { type "b" inner Inner }
        class Row { items (A | B)[] }

        function main() -> int {
            let rows = [
                Row { items: [A { type: "a", inner: Inner { val: 1 } }, B { type: "b", inner: Inner { val: 2 } }] },
                Row { items: [B { type: "b", inner: Inner { val: 4 } }, A { type: "a", inner: Inner { val: 8 } }] }
            ];
            let sum = 0;
            for let _: Row { items } in rows {
                for let _: A { inner: Inner { val } } | B { inner: Inner { val } } in items {
                    sum += val;
                }
            }
            sum
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(15)));
}

#[tokio::test]
async fn for_in_nested_top_level_split_torture() {
    let output = baml_test!(
        r#"
        class A { type "a" val int }
        class B { type "b" val int }
        class Group { label string, items (A | B)[] }

        function main() -> int {
            let groups = [
                Group { label: "g1", items: [A { type: "a", val: 10 }, B { type: "b", val: 20 }] },
                Group { label: "g2", items: [B { type: "b", val: 30 }] }
            ];
            let total = 0;
            for let _: Group { label, items } in groups {
                for let z: A { val } | let z: B { val } in items {
                    total += val;
                }
            }
            total
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(60)));
}

#[tokio::test]
async fn for_in_triple_nested_or_with_break() {
    let output = baml_test!(
        r#"
        class A { type "a" val int }
        class B { type "b" val int }

        function main() -> int {
            let matrix: (A | B)[][] = [
                [A { type: "a", val: 1 }, B { type: "b", val: 2 }],
                [B { type: "b", val: 100 }, A { type: "a", val: 3 }],
                [A { type: "a", val: 4 }, A { type: "a", val: 5 }]
            ];
            let sum = 0;
            for let row in matrix {
                for let _: A { val } | B { val } in row {
                    if val >= 100 {
                        break;
                    }
                    sum += val;
                }
            }
            sum
        }
    "#
    );
    // row0: 1+2=3, row1: break at 100 (0), row2: 4+5=9 → 12
    assert_eq!(output.result, Ok(BexExternalValue::Int(12)));
}

#[tokio::test]
async fn for_in_nested_chain_bind_or_match_combo() {
    let output = baml_test!(
        r#"
        class Cat { type "cat" name string }
        class Dog { type "dog" name string }
        class Owner { pet Cat | Dog }

        function main() -> string {
            let owners = [
                Owner { pet: Cat { type: "cat", name: "Milo" } },
                Owner { pet: Dog { type: "dog", name: "Buddy" } },
                Owner { pet: Cat { type: "cat", name: "Luna" } }
            ];
            let result = "";
            for let _: Owner { pet } in owners {
                let label = match (pet) {
                    let c: Cat { name } => "cat:" + name,
                    let d: Dog { name } => "dog:" + name
                };
                if result.length() > 0 {
                    result = result + ",";
                }
                result = result + label;
            }
            result
        }
    "#
    );
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String(
            "cat:Milo,dog:Buddy,cat:Luna".into()
        ))
    );
}

#[tokio::test]
async fn for_in_triple_bind_chain() {
    let output = baml_test!(
        r#"
        function main() -> int {
            let items = [10, 20, 30];
            let sum = 0;
            for let x: let y: let z in items {
                sum += x + y + z;
            }
            sum
        }
    "#
    );
    // each iteration: x=y=z=val, so 3*10 + 3*20 + 3*30 = 180
    assert_eq!(output.result, Ok(BexExternalValue::Int(180)));
}

#[tokio::test]
async fn for_in_destructure_then_re_destructure_in_body() {
    let output = baml_test!(
        r#"
        class Inner { val int }
        class Outer { inner Inner, tag string }

        function main() -> int {
            let items = [
                Outer { inner: Inner { val: 5 }, tag: "a" },
                Outer { inner: Inner { val: 7 }, tag: "bb" }
            ];
            let sum = 0;
            for let _: Outer { inner, tag } in items {
                let _: Inner { val } = inner;
                sum += val + tag.length();
            }
            sum
        }
    "#
    );
    // (5+1) + (7+2) = 15
    assert_eq!(output.result, Ok(BexExternalValue::Int(15)));
}

#[tokio::test]
async fn for_in_shadow_binding_inside_body() {
    let output = baml_test!(
        r#"
        class Item { val int }

        function main() -> int {
            let items = [Item { val: 10 }, Item { val: 20 }];
            let sum = 0;
            for let x: Item { val } in items {
                let val = val * 2;
                let x = 999;
                sum += val + x;
            }
            sum
        }
    "#
    );
    // iter0: val=20, x=999 → 1019. iter1: val=40, x=999 → 1039. total=2058
    assert_eq!(output.result, Ok(BexExternalValue::Int(2058)));
}

#[tokio::test]
async fn match_inside_match_or_destructure() {
    let output = baml_test!(
        r#"
        class Inner { val int }
        class A { type "a" inner Inner }
        class B { type "b" inner Inner }
        class Wrapper { item A | B }

        function main() -> int {
            let w = Wrapper { item: B { type: "b", inner: Inner { val: 42 } } };
            match (w) {
                Wrapper { item } => match (item) {
                    A { inner: Inner { val } } | B { inner: Inner { val } } => val
                }
            }
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(42)));
}

#[tokio::test]
async fn for_in_mutate_destructured_binding() {
    let output = baml_test!(
        r#"
        class Pair { a int, b int }

        function main() -> int {
            let items = [Pair { a: 1, b: 2 }, Pair { a: 3, b: 4 }, Pair { a: 5, b: 6 }];
            let running = 0;
            for let _: Pair { a, b } in items {
                running += a;
                a = a + b;
                running += a;
            }
            running
        }
    "#
    );
    // iter0: running+=1(=1), a=3, running+=3(=4)
    // iter1: running+=3(=7), a=7, running+=7(=14)
    // iter2: running+=5(=19), a=11, running+=11(=30)
    assert_eq!(output.result, Ok(BexExternalValue::Int(30)));
}

#[tokio::test]
async fn for_in_triple_wildcard_chain() {
    let output = baml_test!(
        r#"
        class Item { val int }

        function main() -> int {
            let items = [Item { val: 3 }, Item { val: 7 }];
            let sum = 0;
            for let x: _: _: Item { val } in items {
                sum += val;
            }
            sum
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(10)));
}

#[tokio::test]
async fn for_in_or_with_continue_guard() {
    let output = baml_test!(
        r#"
        class A { type "a" val int }
        class B { type "b" val int }

        function main() -> int {
            let items: (A | B)[] = [
                A { type: "a", val: 1 },
                B { type: "b", val: 2 },
                A { type: "a", val: 100 },
                B { type: "b", val: 3 },
                A { type: "a", val: 200 }
            ];
            let sum = 0;
            for let _: A { val } | B { val } in items {
                if val > 50 {
                    continue;
                }
                sum += val;
            }
            sum
        }
    "#
    );
    // 1 + 2 + 3 = 6 (skip 100 and 200)
    assert_eq!(output.result, Ok(BexExternalValue::Int(6)));
}

#[tokio::test]
async fn for_in_destructure_pass_to_helper_with_match() {
    let output = baml_test!(
        r#"
        class Cat { type "cat" name string }
        class Dog { type "dog" name string }
        class Owner { pets (Cat | Dog)[] }

        function classify(pet: Cat | Dog) -> string {
            match (pet) {
                Cat { name } => "c:" + name,
                Dog { name } => "d:" + name
            }
        }

        function main() -> string {
            let owners = [
                Owner { pets: [Cat { type: "cat", name: "Milo" }, Dog { type: "dog", name: "Rex" }] },
                Owner { pets: [Dog { type: "dog", name: "Buddy" }] }
            ];
            let result = "";
            for let _: Owner { pets } in owners {
                for let pet in pets {
                    if result.length() > 0 {
                        result = result + ",";
                    }
                    result = result + classify(pet);
                }
            }
            result
        }
    "#
    );
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("c:Milo,d:Rex,d:Buddy".into()))
    );
}
