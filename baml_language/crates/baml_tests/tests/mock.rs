//! Tests for `baml.mock.Mock` — VM-level function-override interception.

use baml_tests::baml_test;
use bex_engine::BexExternalValue;

/// Inside `scope(body)`, direct calls to the original user-defined function
/// are redirected to the replacement. Outside, the original runs.
#[tokio::test]
async fn mock_scope_intercepts_user_function() {
    let output = baml_test!(
        r#"
        function double(n: int) -> int { n * 2 }
        function triple(n: int) -> int { n * 3 }

        function call_double(n: int) -> int { double(n) }

        function main() -> int {
            let m = baml.mock.Mock.new(double, triple);
            let inside = m.scope<int, never>(() -> int throws never { call_double(5) });
            let outside = call_double(5);
            inside * 100 + outside
        }
    "#
    );

    // inside = triple(5) = 15 ; outside = double(5) = 10 ; total = 1510
    assert_eq!(output.result, Ok(BexExternalValue::Int(1510)));
}

/// A multi-arg function can be mocked too — `T` is opaque to `Mock`, so the
/// arity is whatever the bound function value carries.
#[tokio::test]
async fn mock_scope_handles_multi_arg_function() {
    let output = baml_test!(
        r#"
        function add(a: int, b: int) -> int { a + b }
        function multiply(a: int, b: int) -> int { a * b }

        function call_add(a: int, b: int) -> int { add(a, b) }

        function main() -> int {
            let m = baml.mock.Mock.new(add, multiply);
            m.scope<int, never>(() -> int throws never { call_add(6, 7) })
        }
    "#
    );

    // inside scope: add is replaced by multiply ; multiply(6, 7) = 42
    assert_eq!(output.result, Ok(BexExternalValue::Int(42)));
}

/// Mocking a SysOp builtin (`baml.http.fetch`) inside `scope` returns the
/// replacement's response without performing any network I/O.
#[tokio::test]
async fn mock_scope_intercepts_sysop_fetch() {
    let output = baml_test!(
        r#"
        function main() -> string throws unknown {
            let fake = (url: string) -> baml.http.Response throws baml.errors.Io | baml.errors.Timeout {
                baml.http.Response {
                    status_code: 200,
                    headers: {},
                    url: url,
                    _body: "fake body",
                }
            }
            let m = baml.mock.Mock.new(baml.http.fetch, fake);
            m.scope<string, unknown>(() -> string throws unknown {
                let r = baml.http.fetch("http://does-not-exist.invalid/")
                r.url
            })
        }
    "#
    );

    assert_eq!(
        output.result,
        Ok(BexExternalValue::String(
            "http://does-not-exist.invalid/".to_string()
        ))
    );
}

/// `replace` swaps the implementation that `scope` will install.
#[tokio::test]
async fn mock_replace_changes_active_impl() {
    let output = baml_test!(
        r#"
        function original(n: int) -> int { n + 1 }
        function call_original(n: int) -> int { original(n) }

        function main() -> int {
            let m = baml.mock.Mock.new(
                original,
                (n: int) -> int { n + 100 },
            );
            m.replace((n: int) -> int { n + 1000 });
            m.scope<int, never>(() -> int throws never { call_original(5) })
        }
    "#
    );

    // replacement was overridden to `n + 1000`; 5 + 1000 = 1005
    assert_eq!(output.result, Ok(BexExternalValue::Int(1005)));
}

/// `call_count` ticks up automatically for each intercepted call inside
/// `scope` — no need for a user-written counter inside the replacement.
#[tokio::test]
async fn mock_scope_auto_increments_call_count() {
    let output = baml_test!(
        r#"
        function double(n: int) -> int { n * 2 }
        function triple(n: int) -> int { n * 3 }
        function call_double(n: int) -> int { double(n) }

        function main() -> int {
            let m = baml.mock.Mock.new(double, triple)
            let inside = m.scope<int, never>(() -> int throws never {
                call_double(1) + call_double(2) + call_double(3)
            })
            // After scope: replacement ran 3 times, call_count == 3.
            // call_double(4) outside the scope hits the real `double`,
            // returns 8, and does NOT bump call_count.
            let outside = call_double(4)
            inside * 1000 + outside * 100 + m.call_count
        }
    "#
    );

    // inside = 3 + 6 + 9 = 18 ; outside = 8 ; call_count = 3
    // result = 18_000 + 800 + 3 = 18_803
    assert_eq!(output.result, Ok(BexExternalValue::Int(18_803)));
}

/// Sanity: a direct method call (`c.method(arg)`) is intercepted today.
/// This is the common case and the compiler appears to lower it through a
/// path that already consults `mock_stack`. Kept as a regression guard so
/// any future refactor that breaks this surfaces immediately.
#[tokio::test]
async fn mock_intercepts_direct_method_call() {
    let output = baml_test!(
        r#"
        class Counter {
          function bump(self, n: int) -> int { n + 1 }
        }

        function main() -> int {
            let c = Counter {}
            let m = baml.mock.Mock.new(c.bump, (n: int) -> int { 99 })
            m.scope<int, never>(() -> int throws never { c.bump(10) })
        }
    "#
    );

    assert_eq!(output.result, Ok(BexExternalValue::Int(99)));
}

/// `Mock.new(c1.bump, …)` should be **per-instance**: calls on `c1` get
/// the replacement; calls on a different instance `c2` still run the
/// original method. The mock entry remembers the receiver from the
/// `BoundMethod` target.
#[tokio::test]
async fn mock_per_instance_via_bound_method_target() {
    let output = baml_test!(
        r#"
        class Counter {
          function bump(self, n: int) -> int { n + 1 }
        }

        function main() -> int {
            let c1 = Counter {}
            let c2 = Counter {}
            let m = baml.mock.Mock.new(c1.bump, (n: int) -> int { 99 })
            m.scope<int, never>(() -> int throws never {
                c1.bump(10) + c2.bump(10)
            })
        }
    "#
    );

    // c1.bump intercepted (99); c2.bump original (11). Sum = 110.
    assert_eq!(output.result, Ok(BexExternalValue::Int(110)));
}

/// `Mock.new(Counter.bump, …)` (class-method-as-value) should be
/// **class-wide**: calls on any `Counter` instance route through the
/// replacement. Mock entry stores `receiver = None`, so the lookup
/// matches every call regardless of receiver identity.
#[tokio::test]
async fn mock_class_wide_via_function_target() {
    let output = baml_test!(
        r#"
        class Counter {
          function bump(self, n: int) -> int { n + 1 }
        }

        function main() -> int {
            let c1 = Counter {}
            let c2 = Counter {}
            let m = baml.mock.Mock.new(Counter.bump, (self: Counter, n: int) -> int { 99 })
            m.scope<int, never>(() -> int throws never {
                c1.bump(10) + c2.bump(10)
            })
        }
    "#
    );

    // Both instances intercepted; 99 + 99 = 198.
    assert_eq!(output.result, Ok(BexExternalValue::Int(198)));
}

/// `Instruction::CallIndirect` / `OpCode::CallIndirect`'s BoundMethod
/// branch must consult `mock_stack`: stashing `c.method` in a local and
/// calling it indirectly should still hit the mock.
#[tokio::test]
async fn mock_intercepts_indirect_bound_method_call() {
    let output = baml_test!(
        r#"
        class Counter {
          function bump(self, n: int) -> int { n + 1 }
        }

        function main() -> int {
            let c = Counter {}
            let m = baml.mock.Mock.new(c.bump, (n: int) -> int { 99 })
            m.scope<int, never>(() -> int throws never {
                let bound = c.bump;
                bound(10)
            })
        }
    "#
    );

    assert_eq!(output.result, Ok(BexExternalValue::Int(99)));
}

/// Native helpers like `array.map` invoke their callback via
/// `NativeCallResult::YieldToCall`. That path must also consult
/// `mock_stack` so a mocked target passed as a callback gets the
/// replacement.
#[tokio::test]
async fn mock_intercepts_yield_to_call_callback() {
    let output = baml_test!(
        r#"
        function double(n: int) -> int { n * 2 }
        function triple(n: int) -> int { n * 3 }

        function main() -> int {
            let m = baml.mock.Mock.new(double, triple)
            m.scope<int, unknown>(() -> int throws unknown {
                // Pass `double` directly so `array.map`'s native helper
                // dispatches it via YieldToCall — the path that today
                // skips the mock check.
                [1, 2, 3, 4, 5].map(double)
                    .reduce((acc: int, x: int) -> int { acc + x }, 0)
            })
        }
    "#
    );

    // Triple applied to [1..=5] sums to 3+6+9+12+15 = 45.
    // Currently observed: 30 (double ran — mock did not intercept).
    assert_eq!(output.result, Ok(BexExternalValue::Int(45)));
}

/// GC stress: a Closure replacement that captures a heap-allocated cell
/// (the counter) is pushed onto the mock stack, and the body allocates
/// aggressively to force collection while the override is active. The
/// captured cell + the closure itself must stay reachable for the call to
/// dispatch correctly.
#[tokio::test]
async fn mock_scope_survives_gc_under_alloc_pressure() {
    let output = baml_test!(
        r#"
        function target(n: int) -> int { n + 1 }
        function call_target(n: int) -> int { target(n) }

        function main() -> int {
            // Build a Mock with a Closure replacement that captures `counter`.
            // The captured cell lives on the heap; the closure itself is
            // heap-allocated. Both must stay reachable across GC cycles
            // while pushed onto the mock stack inside `scope`.
            let counter = 0;
            let replacement = (n: int) -> int {
                counter = counter + 1;
                n * 100
            };
            let m = baml.mock.Mock.new(target, replacement);

            let sum = m.scope<int, unknown>(() -> int throws unknown {
                let acc = 0;
                let i = 0;
                // Allocate aggressively + yield periodically via sleep(0).
                // The sleep is async, which hands control back to the engine
                // and lets `maybe_collect_garbage` run an actual collection
                // while the mock is on the stack.
                while (i < 5000) {
                    let scratch = [
                        [i, i + 1, i + 2],
                        [i + 3, i + 4, i + 5],
                        [i + 6, i + 7, i + 8],
                    ];
                    acc = acc + call_target(scratch[0][0]);
                    if (i % 100 == 0) {
                        let _ = baml.sys.sleep(0);
                    } else {
                        let _ = 0;
                    }
                    i = i + 1;
                }
                acc
            });

            // Reference `counter` after scope to keep its cell alive and to
            // sanity-check the replacement was actually invoked 20_000 times.
            sum + counter
        }
        "#
    );

    // Replacement does `n * 100` for n = 0..5000.
    // sum_{i=0}^{4999} i*100 = 100 * (4999 * 5000 / 2) = 1_249_750_000
    // counter ends at 5000 → total = 1_249_755_000.
    assert_eq!(output.result, Ok(BexExternalValue::Int(1_249_755_000)));
}

/// The override is popped even when the body throws.
#[tokio::test]
async fn mock_scope_pops_on_throw() {
    let output = baml_test!(
        r#"
        function original(n: int) -> int { n + 1 }
        function call_original(n: int) -> int { original(n) }

        function main() -> int {
            let m = baml.mock.Mock.new(
                original,
                (n: int) -> int { n + 100 },
            );
            let _ = {
                m.scope<int, unknown>(() -> int throws unknown {
                    throw "boom"
                })
            } catch_all (e) {
                _ => 0,
            }
            // After the scope unwinds, `original` should be back to normal.
            call_original(7)
        }
    "#
    );

    assert_eq!(output.result, Ok(BexExternalValue::Int(8)));
}
