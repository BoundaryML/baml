//! BEP-058 function mocking — TORTURE: sad paths & abuse.
//!
//! These push the mock mechanism to its breaking point with extreme and
//! defensive cases: empty/duplicate/mixed array scopes, mocking the mock
//! machinery (must reject), call_count read in degenerate positions, replacements
//! that return mocks, optional-returning targets, deep same-mock re-entry, and
//! LIFO tie-breaking among distinct mocks on one target. Predictions are traced
//! from the BEP-058 spec and the implementation (`mock_dispatch`, `__enter` /
//! `__exit`, `scope_mock_ptrs`, the recursion guard).

use baml_tests::baml_test;
use bex_engine::BexExternalValue;

/// scope([], body): an empty array activates nothing — calls inside hit the real
/// function and no mock can fire; the body still runs to completion.
#[tokio::test]
async fn torture_abuse_sadpath_01_empty_array_scope_is_noop() {
    let output = baml_test!(
        r#"
        function target(x: int) -> int { x * 2 }

        function main() -> int {
            let m = baml.mock.new(target);
            m.replace((x: int) -> int { 99 });
            let r = 0;
            // The mock object exists but is NOT in the array, so nothing is active.
            baml.mock.scope([], () -> void {
                r = target(5);                 // real -> 10 (no mock activated)
            });
            r * 10 + m.call_count              // 10*10 + 0 (never fired) = 100
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(100)));
}

/// scope([a, a], body): the same mock listed twice. `__enter` pushes it twice but
/// only the first activation resets the counter; `mock_dispatch` dedups via `seen`
/// so the call is observed once. `__exit` pops both copies cleanly, so after the
/// scope the real function is restored.
#[tokio::test]
async fn torture_abuse_sadpath_02_duplicate_mock_in_array_counts_once() {
    let output = baml_test!(
        r#"
        function target() -> int { 1 }

        function main() -> int {
            let a = baml.mock.new(target);
            a.replace(() -> int { 7 });
            let inside = 0;
            baml.mock.scope([a, a], () -> void {
                inside = target();             // dedup -> fires once -> 7, count 1
            });
            let after = target();              // both copies popped -> real 1
            inside * 1000 + after * 100 + a.call_count  // 7000 + 100 + 1 = 7101
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(7101)));
}

/// scope([a, b], body) where a and b mock DIFFERENT targets: each fires only for
/// its own target; the calls do not cross-contaminate, and both deactivate.
#[tokio::test]
async fn torture_abuse_sadpath_03_array_mocks_on_different_targets() {
    let output = baml_test!(
        r#"
        function foo() -> int { 1 }
        function bar() -> int { 10 }

        function main() -> int {
            let a = baml.mock.new(foo);
            a.replace(() -> int { 2 });
            let b = baml.mock.new(bar);
            b.replace(() -> int { 20 });
            let rf = 0;
            let rb = 0;
            baml.mock.scope([a, b], () -> void {
                rf = foo();                    // a -> 2
                rb = bar();                    // b -> 20
            });
            // both popped; counts: a=1, b=1
            rf * 1000 + rb * 10 + a.call_count + b.call_count  // 2000 + 200 + 2 = 2202
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(2202)));
}

/// scope with an array element that is NOT a mock: a plain int slips into the
/// array. `scope_mock_ptrs` filters non-Mock elements (filter_map), so the int is
/// silently ignored and only the real mock activates. (Whether this type-checks is
/// itself under test; if the array must be homogeneously `Mock`, a non-mock
/// element should be rejected at compile time -> Err.)
#[tokio::test]
async fn torture_abuse_sadpath_04_array_with_non_mock_element() {
    let output = baml_test!(
        r#"
        function target() -> int { 1 }

        function main() -> int {
            let m = baml.mock.new(target);
            m.replace(() -> int { 5 });
            let r = 0;
            // A heterogeneous array [Mock, int] — ill-typed for Mock[].
            baml.mock.scope([m, 42], () -> void {
                r = target();
            });
            r + m.call_count
        }
        "#
    );
    // Heterogeneous array element should be rejected by the type checker.
    assert!(
        output.result.is_err(),
        "a non-Mock array element must be rejected, got {:?}",
        output.result
    );
}

/// Mocking `baml.mock.new` itself is rejected: its key name starts with
/// `baml.mock.`, which `__new` refuses (non-mockable runtime internal) -> Err.
#[tokio::test]
async fn torture_abuse_sadpath_05_mock_new_rejected() {
    let output = baml_test!(
        r#"
        function main() -> int {
            let m = baml.mock.new(baml.mock.new);   // mocking new itself
            0
        }
        "#
    );
    assert!(
        output.result.is_err(),
        "mocking baml.mock.new must be rejected, got {:?}",
        output.result
    );
}

/// Mocking `m.replace` (a bound method on a Mock) is rejected: its key is
/// `Instance(m, "baml.mock.Mock.replace")`, whose name starts with `baml.mock.`,
/// so `__new` refuses it -> Err.
#[tokio::test]
async fn torture_abuse_sadpath_06_mock_replace_rejected() {
    let output = baml_test!(
        r#"
        function target() -> int { 0 }

        function main() -> int {
            let m = baml.mock.new(target);
            let evil = baml.mock.new(m.replace);    // mocking replace itself
            0
        }
        "#
    );
    assert!(
        output.result.is_err(),
        "mocking m.replace must be rejected, got {:?}",
        output.result
    );
}

/// call_count read on a never-activated mock is 0: a mock created but never passed
/// to any scope must report zero invocations, even though the target is called.
#[tokio::test]
async fn torture_abuse_sadpath_07_call_count_never_activated_is_zero() {
    let output = baml_test!(
        r#"
        function target() -> int { 1 }

        function main() -> int {
            let m = baml.mock.new(target);
            m.replace(() -> int { 99 });
            let _ = target();                  // called, but mock never activated
            let _ = target();
            m.call_count                       // never in scope -> 0
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(0)));
}

/// call_count read AFTER the scope exits retains the in-scope total (it is reset
/// only on the next fresh entry), and calls made after exit do not bump it.
#[tokio::test]
async fn torture_abuse_sadpath_08_call_count_persists_after_scope_exit() {
    let output = baml_test!(
        r#"
        function target() -> int { 1 }

        function main() -> int {
            let m = baml.mock.new(target);
            m.replace(() -> int { 0 });
            baml.mock.scope(m, () -> void {
                let _ = target();              // count 1
                let _ = target();              // count 2
            });
            let _ = target();                  // after exit: NOT counted (real)
            let _ = target();
            m.call_count                       // still 2 from the scope
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(2)));
}

/// Re-replace mid-program then read: calling `.replace` twice swaps the stand-in;
/// the later replacement wins for calls after it (the BEP says `.replace` takes
/// effect immediately). count accumulates across both replacements in one scope.
#[tokio::test]
async fn torture_abuse_sadpath_09_re_replace_then_read() {
    let output = baml_test!(
        r#"
        function target() -> int { 1 }

        function main() -> int {
            let m = baml.mock.new(target);
            m.replace(() -> int { 10 });
            let first = 0;
            let second = 0;
            baml.mock.scope(m, () -> void {
                first = target();              // 10 (count 1)
                m.replace(() -> int { 20 });   // swap mid-scope
                second = target();             // 20 (count 2)
            });
            first * 1000 + second * 10 + m.call_count  // 10000 + 200 + 2 = 10202
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(10202)));
}

/// A replacement that RETURNS a Mock value: the replacement constructs and returns
/// a fresh `baml.mock.new(...)` object. We observe it indirectly via its
/// call_count (0, never activated). Stresses value-flow of Mock objects out of a
/// replacement without corrupting the mock table.
#[tokio::test]
async fn torture_abuse_sadpath_10_replacement_returns_a_mock() {
    let output = baml_test!(
        r#"
        function helper() -> int { 1 }
        function makesMock() -> baml.mock.Mock<() -> int> {
            baml.mock.new(helper)
        }

        function main() -> int {
            let m = baml.mock.new(makesMock);
            m.replace(() -> baml.mock.Mock<() -> int> { baml.mock.new(helper) });
            let produced_count = -1;
            baml.mock.scope(m, () -> void {
                let inner = makesMock();        // replacement -> a fresh Mock
                produced_count = inner.call_count;  // brand-new mock -> 0
            });
            produced_count * 10 + m.call_count  // 0*10 + 1 = 1
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(1)));
}

/// Mocking a function that RETURNS a mock-typed value, then mocking it so the
/// returned Mock comes from the replacement. The outer code never activates the
/// inner mock, so its count stays 0; the outer mock fired once.
#[tokio::test]
async fn torture_abuse_sadpath_11_target_returns_mock_typed_value() {
    let output = baml_test!(
        r#"
        function leaf() -> int { 7 }
        function getMock() -> baml.mock.Mock<() -> int> { baml.mock.new(leaf) }

        function main() -> int {
            let outer = baml.mock.new(getMock);
            outer.replace(() -> baml.mock.Mock<() -> int> {
                let mk = baml.mock.new(leaf);
                mk.replace(() -> int { 123 });   // set but never scoped
                mk
            });
            let cnt = -1;
            baml.mock.scope(outer, () -> void {
                let produced = getMock();        // outer replacement -> Mock
                cnt = produced.call_count;       // never activated -> 0
            });
            cnt * 100 + outer.call_count         // 0 + 1 = 1
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(1)));
}

/// Mocking an OPTIONAL-returning function (post-merge: `T | null`). The
/// replacement returns null in scope; outside it the real (non-null) value comes
/// back. Optionals are consumed with `match` (null arm). Stresses a replacement
/// whose return type is a union (`int?`) flowing through the dispatch hook.
#[tokio::test]
async fn torture_abuse_sadpath_12_mock_optional_returning_function() {
    let output = baml_test!(
        r#"
        function maybe(x: int) -> int? { x }

        function unwrap(o: int?) -> int {
            match (o) {
                let v: int => v
                null => -1
            }
        }

        function main() -> int {
            let m = baml.mock.new(maybe);
            m.replace((x: int) -> int? { null });   // replacement returns null
            let inside = maybe(5);
            baml.mock.scope(m, () -> void {
                inside = maybe(5);                   // mocked -> null
            });
            let outside = maybe(9);                  // real -> 9
            unwrap(inside) * 100 + unwrap(outside)  // -1*100 + 9 = -91
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(-91)));
}

/// A bound-method callable stored in an array element, mocked through that
/// element, and called indirectly via the array. The instance key must match the
/// receiver even when the call goes through `CallIndirect` on a BoundMethod value.
#[tokio::test]
async fn torture_abuse_sadpath_13_bound_method_in_array_called_indirectly() {
    let output = baml_test!(
        r#"
        class C {
          v int
          function get(self) -> int { self.v }
        }

        function main() -> int {
            let c = C { v: 3 };
            let fns = [c.get];                 // bound method type-erased into array
            let m = baml.mock.new(fns[0]);     // Instance(c, "...get")
            m.replace(() -> int { 42 });
            let inside = 0;
            baml.mock.scope(m, () -> void {
                inside = fns[0]();             // CallIndirect on BoundMethod -> 42
            });
            let after = fns[0]();              // real -> 3
            inside * 100 + after + m.call_count   // 4200 + 3 + 1 = 4204
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(4204)));
}

/// A Mock stored in a CLASS FIELD, then read back out and scoped. The mock must
/// survive being stored in / loaded from an instance field (write barrier + GC
/// rooting) and still activate correctly.
#[tokio::test]
async fn torture_abuse_sadpath_14_mock_stored_in_class_field() {
    let output = baml_test!(
        r#"
        function target() -> int { 1 }

        class Holder {
          m baml.mock.Mock<() -> int>
        }

        function main() -> int {
            let mk = baml.mock.new(target);
            mk.replace(() -> int { 88 });
            let h = Holder { m: mk };
            let inside = 0;
            baml.mock.scope(h.m, () -> void {      // scope the field-held mock
                inside = target();                 // -> 88
            });
            inside * 10 + h.m.call_count           // 880 + 1 = 881
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(881)));
}

/// Two DISTINCT mocks on the same target, both active, with explicit LIFO winner.
/// Both have replacements (no spy), so the innermost (last-activated) one claims
/// the call and short-circuits — the outer one is NOT counted (the inner returns
/// before delegating). Verifies LIFO precedence among same-key replacements and
/// that only the head observes the call.
#[tokio::test]
async fn torture_abuse_sadpath_15_two_mocks_same_target_lifo_winner() {
    let output = baml_test!(
        r#"
        function target() -> int { 0 }

        function main() -> int {
            let outerM = baml.mock.new(target);
            outerM.replace(() -> int { 1 });
            let innerM = baml.mock.new(target);
            innerM.replace(() -> int { 2 });
            let r = 0;
            baml.mock.scope(outerM, () -> void {
                baml.mock.scope(innerM, () -> void {
                    r = target();                  // innermost wins -> 2
                });
            });
            // inner claimed and returned: inner count 1, outer count 0
            r * 100 + innerM.call_count * 10 + outerM.call_count  // 200 + 10 + 0 = 210
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(210)));
}

/// Calling a mocked target ZERO times inside its scope: the scope opens and closes
/// without touching the target, so call_count stays 0 (entry reset, no firing).
#[tokio::test]
async fn torture_abuse_sadpath_16_zero_calls_in_scope() {
    let output = baml_test!(
        r#"
        function target() -> int { 1 }

        function main() -> int {
            let m = baml.mock.new(target);
            m.replace(() -> int { 99 });
            let sentinel = 0;
            baml.mock.scope(m, () -> void {
                sentinel = 5;                  // never calls target
            });
            sentinel * 10 + m.call_count       // 50 + 0 = 50
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(50)));
}

/// Nesting scope of the SAME mock 5 levels deep: re-entering an already-active
/// mock must NOT reset call_count (only the first activation resets). One call per
/// level accumulates to 5 across the nesting; `seen` dedup ensures each call is
/// counted once despite the mock appearing 5 times in the stack.
#[tokio::test]
async fn torture_abuse_sadpath_17_same_mock_nested_five_deep_accumulates() {
    let output = baml_test!(
        r#"
        function target() -> int { 0 }

        function main() -> int {
            let m = baml.mock.new(target);
            m.replace(() -> int { 0 });
            baml.mock.scope(m, () -> void {
                let _ = target();                       // count 1
                baml.mock.scope(m, () -> void {
                    let _ = target();                   // count 2
                    baml.mock.scope(m, () -> void {
                        let _ = target();               // count 3
                        baml.mock.scope(m, () -> void {
                            let _ = target();           // count 4
                            baml.mock.scope(m, () -> void {
                                let _ = target();       // count 5
                            });
                        });
                    });
                });
            });
            m.call_count                                // 5, never reset mid-nest
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(5)));
}

/// A replacement that calls the target by name in a TIGHT loop: the recursion
/// guard (mock_suppress) must let each by-name re-entry step exactly one layer
/// down to the real function, so this terminates. If the guard fails this
/// infinite-loops (the runner detects the hang).
#[tokio::test]
async fn torture_abuse_sadpath_18_replacement_calls_original_in_loop_terminates() {
    let output = baml_test!(
        r#"
        function f(x: int) -> int { x + 1 }

        function main() -> int {
            let m = baml.mock.new(f);
            // Replacement re-enters f by name; the recursion guard must resolve
            // that to the REAL f (one step down), not back into the replacement.
            m.replace((x: int) -> int { f(x) * 2 });   // 2*(x+1)
            let total = 0;
            baml.mock.scope(m, () -> void {
                let i = 0;
                while (i < 3) {
                    total = total + f(i);              // 2*(i+1): 2 + 4 + 6 = 12
                    i = i + 1;
                }
            });
            total * 10 + m.call_count                  // 120 + 3 (only outer calls) = 123
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(123)));
}

/// A pure-spy array scope where the ONLY element is a spy (no replacement): the
/// original runs and is counted, with no replacement anywhere in the chain.
/// Stresses `mock_dispatch` returning `Spy` (not `Redirect`) from the array path.
#[tokio::test]
async fn torture_abuse_sadpath_19_array_scope_pure_spy_only() {
    let output = baml_test!(
        r#"
        function target(x: int) -> int { x * 3 }

        function main() -> int {
            let s = baml.mock.new(target);     // pure spy, no replace
            let r = 0;
            baml.mock.scope([s], () -> void {
                r = target(4);                 // runs original -> 12, counted
                let _ = target(1);             // counted
            });
            r * 10 + s.call_count              // 120 + 2 = 122
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(122)));
}

/// Mocking `baml.mock.scope` itself (the activation primitive) is rejected: its
/// key name starts with `baml.mock.` -> `__new` refuses it -> Err. (Mirrors the
/// existing internals test but as a standalone abuse case alongside new/replace.)
#[tokio::test]
async fn torture_abuse_sadpath_20_mock_scope_rejected() {
    let output = baml_test!(
        r#"
        function main() -> int {
            let m = baml.mock.new(baml.mock.scope);   // mocking the activation primitive
            0
        }
        "#
    );
    assert!(
        output.result.is_err(),
        "mocking baml.mock.scope must be rejected, got {:?}",
        output.result
    );
}

/// A single non-`Mock` scope argument (`scope(42)`) must be rejected, not silently
/// no-op. The array guard catches `[m, 42]`, but a bare non-Mock top-level value
/// skips `scope_mock_ptrs` (which returns empty) and would otherwise succeed.
#[tokio::test]
async fn torture_abuse_sadpath_21_single_non_mock_scope_arg_rejected() {
    let output = baml_test!(
        r#"
        function target() -> int { 1 }

        function main() -> int {
            let m = baml.mock.new(target);
            m.replace(() -> int { 5 });
            baml.mock.scope(42, () -> void {   // 42 is not a Mock
                let _ = target();
            });
            0
        }
        "#
    );
    assert!(
        output.result.is_err(),
        "a single non-Mock scope argument must be rejected, got {:?}",
        output.result
    );
}
