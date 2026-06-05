//! BEP-058: function mocking — regression tests.
//!
//! Slice 1 (free functions): `baml.mock.new` creates a mock of a free function,
//! `.replace` sets a stand-in, `baml.mock.scope` activates it for the body, calls
//! inside the scope hit the replacement, calls outside do not, and `call_count`
//! tracks invocations within the scope.

use baml_tests::baml_test;
use bex_engine::BexExternalValue;

/// The replacement is used for calls inside the scope, and the original is
/// restored outside it.
#[tokio::test]
async fn mock_free_function_replacement_used_only_in_scope() {
    let output = baml_test!(
        r#"
        function original(x: int) -> int { x * 2 }

        function main() -> int {
            let m = baml.mock.new(original);
            m.replace((x: int) -> int { 99 });

            let before = original(7);          // outside scope: real -> 14
            let inside = 0;
            baml.mock.scope(m, () -> void {
                inside = original(7);          // inside scope: replacement -> 99
            });
            let after = original(7);           // scope ended: real again -> 14

            before + inside + after            // 14 + 99 + 14 = 127
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(127)));
}

/// `call_count` reflects the number of calls to the mocked function within the
/// scope.
#[tokio::test]
async fn mock_call_count_counts_calls_in_scope() {
    let output = baml_test!(
        r#"
        function original(x: int) -> int { x * 2 }

        function main() -> int {
            let m = baml.mock.new(original);
            m.replace((x: int) -> int { 0 });
            baml.mock.scope(m, () -> void {
                let _ = original(1);
                let _ = original(2);
                let _ = original(3);
            });
            m.call_count
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(3)));
}

/// A named function held in a value and called indirectly (`let g = original;
/// g(7)`) is still intercepted by a mock on that function.
#[tokio::test]
async fn mock_indirect_call_through_function_value() {
    let output = baml_test!(
        r#"
        function original(x: int) -> int { x * 2 }

        function main() -> int {
            let g = original;                  // function value -> indirect call
            let m = baml.mock.new(g);
            m.replace((x: int) -> int { 99 });
            let inside = 0;
            baml.mock.scope(m, () -> void {
                inside = g(7);                 // CallIndirect -> mocked -> 99
            });
            let after = g(7);                  // scope ended -> real -> 14
            inside + after                     // 99 + 14 = 113
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(113)));
}

/// A lambda value (BEP Target #1) is mockable when called through its binding.
#[tokio::test]
async fn mock_indirect_call_through_lambda_value() {
    let output = baml_test!(
        r#"
        function main() -> int {
            let f = (x: int) -> int { x };     // lambda value
            let m = baml.mock.new(f);
            m.replace((x: int) -> int { 99 });
            let r = 0;
            baml.mock.scope(m, () -> void {
                r = f(7);                      // CallIndirect on the lambda -> 99
            });
            r
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(99)));
}

/// Distinct callable kinds stored together in an array keep their own runtime
/// identity: mocking each array element keys it independently (free function vs
/// lambda vs bound method), with no cross-fire.
#[tokio::test]
async fn mock_mixed_callable_array_discriminates_each_kind() {
    let output = baml_test!(
        r#"
        function free_fn() -> int { 1 }

        class C {
          v int
          function m(self) -> int { self.v }
        }

        function main() -> int {
            let lam = () -> int { 2 };
            let c = C { v: 3 };
            // Three distinct callable kinds type-erased into one array.
            let fns = [free_fn, lam, c.m];

            let m0 = baml.mock.new(fns[0]);    // free fn -> Free("...free_fn")
            let m1 = baml.mock.new(fns[1]);    // lambda  -> Free("<lambda...>")
            let m2 = baml.mock.new(fns[2]);    // bound   -> Instance(c, "...m")
            m0.replace(() -> int { 100 });
            m1.replace(() -> int { 200 });
            m2.replace(() -> int { 300 });

            let r = 0;
            baml.mock.scope(m0, () -> void {
                baml.mock.scope(m1, () -> void {
                    baml.mock.scope(m2, () -> void {
                        // each call resolves to its own mock, no collision
                        r = fns[0]() + fns[1]() + fns[2]();   // 100 + 200 + 300
                    });
                });
            });
            r
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(600)));
}

// ─── Slice 2: class / instance method mocks ───────────────────────────────────

/// Mocking via the class name affects every instance.
#[tokio::test]
async fn mock_class_method_affects_all_instances() {
    let output = baml_test!(
        r#"
        class Counter {
          count int
          function bump(self) -> int { self.count + 1 }
        }

        function main() -> int {
            let m = baml.mock.new(Counter.bump);
            m.replace((self: Counter) -> int { -1 });
            let a = Counter { count: 0 };
            let b = Counter { count: 100 };

            let before = a.bump();             // real -> 1
            let ia = 0;
            let ib = 0;
            baml.mock.scope(m, () -> void {
                ia = a.bump();                 // mocked -> -1
                ib = b.bump();                 // mocked -> -1 (all instances)
            });
            let after = a.bump();              // real -> 1

            before + ia + ib + after           // 1 + -1 + -1 + 1 = 0
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(0)));
}

/// Mocking via a single instance affects only that instance.
#[tokio::test]
async fn mock_instance_method_affects_only_that_instance() {
    let output = baml_test!(
        r#"
        class Counter {
          count int
          function bump(self) -> int { self.count + 1 }
        }

        function main() -> int {
            let a = Counter { count: 0 };
            let b = Counter { count: 0 };
            let m = baml.mock.new(a.bump);     // mock only instance `a`
            m.replace(() -> int { -1 });
            let ra = 0;
            let rb = 0;
            baml.mock.scope(m, () -> void {
                ra = a.bump();                 // mocked -> -1
                rb = b.bump();                 // NOT mocked -> 1
            });
            ra + rb                            // -1 + 1 = 0
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(0)));
}

// ─── Slice 3: interface method mocks (all implementors) ───────────────────────

/// Mocking via the interface name affects every implementor (BEP-058 slice 3).
///
/// `new(Animal.speak)` lowers the interface method value to a pooled
/// `Object::InterfaceMethodRef` carrying `user.Animal.speak`, and the dispatch
/// hook maps the concrete callee (`user.Dog.Animal.speak`) back to that key via
/// the implementor registry, so the mock fires for every implementor.
#[tokio::test]
async fn mock_interface_method_affects_all_implementors() {
    let output = baml_test!(
        r#"
        interface Animal {
          function speak(self) -> string
        }
        class Dog {
          implements Animal {
            function speak(self) -> string { "woof" }
          }
        }
        class Cat {
          implements Animal {
            function speak(self) -> string { "meow" }
          }
        }

        function main() -> string {
            let m = baml.mock.new(Animal.speak);
            m.replace((self: Animal) -> string { "[muted]" });
            let d = Dog {};
            let c = Cat {};

            let before = d.speak();            // "woof"
            let id = "";
            let ic = "";
            baml.mock.scope(m, () -> void {
                id = d.speak();                // "[muted]"
                ic = c.speak();                // "[muted]" (all implementors)
            });
            let after = d.speak();             // "woof"

            before + id + ic + after           // "woof[muted][muted]woof"
        }
        "#
    );
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("woof[muted][muted]woof".into()))
    );
}

// ─── Slice 4: generic specialization mocks ────────────────────────────────────

/// Mocking `f<int>` affects only that instantiation, not `f<string>`, and not
/// `f<int>` outside the scope.
#[tokio::test]
async fn mock_generic_specialization_only_that_instantiation() {
    let output = baml_test!(
        r#"
        function identity<T>(x: T) -> T { x }

        function main() -> int {
            let m = baml.mock.new(identity<int>);
            m.replace((x: int) -> int { -1 });
            let r_int = 0;
            let r_str_len = 0;
            baml.mock.scope(m, () -> void {
                r_int = identity<int>(5);                   // mocked -> -1
                r_str_len = identity<string>("hi").length(); // not mocked -> 2
            });
            let outside = identity<int>(5);                 // real -> 5
            r_int + outside + r_str_len                     // -1 + 5 + 2 = 6
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(6)));
}

/// Mocking the bare generic (no type args) covers every instantiation.
#[tokio::test]
async fn mock_bare_generic_covers_all_instantiations() {
    let output = baml_test!(
        r#"
        function identity<T>(x: T) -> T { x }

        function main() -> int {
            let m = baml.mock.new(identity);
            m.replace(<T>(x: T) -> T { x });
            baml.mock.scope(m, () -> void {
                let _ = identity<int>(1);
                let _ = identity<string>("a");
                let _ = identity<int>(2);
            });
            m.call_count                       // all 3 instantiations counted
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(3)));
}

// ─── Slice 6: pure spy (no replacement) ───────────────────────────────────────

/// A mock with no `.replace` is a pure spy: the original runs, but calls count.
#[tokio::test]
async fn mock_pure_spy_runs_original_and_counts() {
    let output = baml_test!(
        r#"
        function original(x: int) -> int { x * 2 }

        function main() -> int {
            let m = baml.mock.new(original);   // no .replace -> pure spy
            let r = 0;
            baml.mock.scope(m, () -> void {
                r = original(5);               // runs original -> 10
                let _ = original(7);           // runs original, counted
            });
            r + m.call_count                   // 10 + 2 = 12
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(12)));
}

/// Nesting a scope of the *same* mock inside itself preserves the call_count
/// accumulated by the outer scope (the redundant re-entry must not reset it).
#[tokio::test]
async fn mock_nested_same_mock_preserves_call_count() {
    let output = baml_test!(
        r#"
        function target() -> int { 0 }

        function main() -> int {
            let m = baml.mock.new(target);
            m.replace(() -> int { 0 });
            baml.mock.scope(m, () -> void {
                let _ = target();                 // count -> 1
                baml.mock.scope(m, () -> void {   // same mock, nested
                    let _ = target();             // count -> 2
                });
                let _ = target();                 // count -> 3
            });
            m.call_count                          // 3 calls in the active extent
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(3)));
}

// ─── Slice 7: unwind — scope pops the mock even when the body throws ───────────

/// If the scope body throws, the mock is still deactivated as the error unwinds
/// through `scope`, so calls after the (caught) scope hit the original.
#[tokio::test]
async fn mock_scope_pops_on_unwind() {
    let output = baml_test!(
        r#"
        function target() -> int { 1 }

        function main() -> int {
            let m = baml.mock.new(target);
            m.replace(() -> int { 99 });
            let caught = 0;
            baml.mock.scope(m, () -> void {
                throw "boom";              // body throws mid-scope
            }) catch (e) {
                _ => { caught = 1 }
            };
            let after = target();          // mock popped on unwind -> real 1, not 99
            caught * 1000 + after          // 1000 + 1 = 1001
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(1001)));
}

/// A pure spy stacked over a replacement is transparent: it observes the call
/// (count bumped) and delegates one step down to the replacement, rather than
/// short-circuiting to the real function.
#[tokio::test]
async fn mock_spy_delegates_to_lower_replacement() {
    let output = baml_test!(
        r#"
        function greeting() -> string { "hello" }

        function main() -> int {
            let a = baml.mock.new(greeting);
            a.replace(() -> string { "A" });
            let s = baml.mock.new(greeting);   // pure spy, no replace, stacked on top
            let r = "";
            baml.mock.scope(a, () -> void {
                baml.mock.scope(s, () -> void {
                    r = greeting();            // spy delegates down -> a's "A"
                });
            });
            // r == "A" (len 1, not the real "hello" len 5); spy and the claiming
            // replacement both observed the call. Buggy short-circuit -> 5,0,_.
            r.length() * 100 + s.call_count * 10 + a.call_count   // 1*100 + 1*10 + 1
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(111)));
}

/// A spy in the middle of a stack does not drop the replacements below it: the
/// call delegates past the spy to the next replacement down.
#[tokio::test]
async fn mock_spy_in_middle_of_stack_is_transparent() {
    let output = baml_test!(
        r#"
        function greeting() -> string { "hello" }

        function main() -> string {
            let a = baml.mock.new(greeting);
            a.replace(() -> string { "A[" + greeting() + "]" });
            let b = baml.mock.new(greeting);   // pure spy in the middle
            let c = baml.mock.new(greeting);
            c.replace(() -> string { "C[" + greeting() + "]" });
            let r = "";
            baml.mock.scope(a, () -> void {
                baml.mock.scope(b, () -> void {
                    baml.mock.scope(c, () -> void {
                        r = greeting();        // C[A[hello]] (spy b transparent)
                    });
                });
            });
            r
        }
        "#
    );
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("C[A[hello]]".into()))
    );
}

// ─── Slice 5: nesting + most-specific precedence ──────────────────────────────

/// Nesting two mocks on the same target: innermost wins, and exiting restores
/// the outer.
#[tokio::test]
async fn mock_nesting_innermost_wins() {
    let output = baml_test!(
        r#"
        function greeting() -> string { "hello" }

        function main() -> string {
            let a = baml.mock.new(greeting);
            a.replace(() -> string { "A" });
            let b = baml.mock.new(greeting);
            b.replace(() -> string { "B" });
            let r1 = "";
            let r2 = "";
            let r3 = "";
            baml.mock.scope(a, () -> void {
                r1 = greeting();               // "A"
                baml.mock.scope(b, () -> void {
                    r2 = greeting();           // "B" (innermost)
                });
                r3 = greeting();               // "A" (b popped)
            });
            r1 + r2 + r3                       // "ABA"
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::String("ABA".into())));
}

/// A replacement calling the target by name steps one layer down the stack
/// (super-style delegation through the resolution chain).
#[tokio::test]
async fn mock_delegation_steps_down_the_chain() {
    let output = baml_test!(
        r#"
        function greeting() -> string { "hello" }

        function main() -> string {
            let a = baml.mock.new(greeting);
            a.replace(() -> string { "A[" + greeting() + "]" });
            let b = baml.mock.new(greeting);
            b.replace(() -> string { "B[" + greeting() + "]" });
            let r = "";
            baml.mock.scope(a, () -> void {
                baml.mock.scope(b, () -> void {
                    r = greeting();    // B[A[hello]]
                });
            });
            r
        }
        "#
    );
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("B[A[hello]]".into()))
    );
}

/// When an instance mock and a class mock both match, the more specific
/// (instance) wins.
#[tokio::test]
async fn mock_precedence_instance_beats_class() {
    let output = baml_test!(
        r#"
        class Counter {
          count int
          function bump(self) -> int { self.count + 1 }
        }

        function main() -> int {
            let cm = baml.mock.new(Counter.bump);
            cm.replace((self: Counter) -> int { 100 });
            let a = Counter { count: 0 };
            let im = baml.mock.new(a.bump);
            im.replace(() -> int { 1 });
            let r = 0;
            baml.mock.scope(cm, () -> void {
                baml.mock.scope(im, () -> void {
                    r = a.bump();              // instance mock wins -> 1
                });
            });
            r
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(1)));
}

// ─── Slice 9: spawn propagation ───────────────────────────────────────────────

/// A `spawn` launched inside an active scope sees the same mocks (BEP
/// Concurrency): the mocked target hits the replacement inside the spawn, and
/// the shared `call_count` includes the spawned call.
#[tokio::test]
async fn mock_spawn_inside_scope_sees_the_mock() {
    let output = baml_test!(
        r#"
        function target() -> int { 1 }

        function main() -> int {
            let m = baml.mock.new(target);
            m.replace(() -> int { 99 });
            let r = 0;
            baml.mock.scope(m, () -> void {
                let f = spawn { target() };    // spawn calls the mocked target
                r = await f;                   // 99 (mock visible in the spawn)
            });
            r * 10 + m.call_count              // 99*10 + 1 (shared count) = 991
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(991)));
}

// ─── Slice 8: recursion guard ─────────────────────────────────────────────────

/// A replacement that calls the original by name resolves to the original (one
/// step down), not back into itself — no infinite recursion.
#[tokio::test]
async fn mock_recursion_guard_replacement_calls_original() {
    let output = baml_test!(
        r#"
        function f(x: int) -> int { x }

        function main() -> int {
            let m = baml.mock.new(f);
            m.replace((x: int) -> int { f(x) + 1 });  // calls original f
            let r = 0;
            baml.mock.scope(m, () -> void {
                r = f(5);                             // f(5)+1 where inner f(5)=5 -> 6
            });
            r
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(6)));
}
