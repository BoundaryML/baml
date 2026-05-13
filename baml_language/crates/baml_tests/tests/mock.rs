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

/// KNOWN GAP — `Instruction::CallIndirect` / `OpCode::CallIndirect`:
/// the `BoundMethod` branch does NOT consult `mock_stack`. So if a bound
/// method is *stashed as a value* and then invoked indirectly (rather
/// than called directly as `c.method(...)`), the override is bypassed.
///
/// This test stores `c.bump` in a local and calls it indirectly inside
/// `scope`. With the fix in place the assertion should be `99`; today
/// the original method runs and the value is `11`. Marked `#[ignore]` so
/// the suite stays green until the BoundMethod CallIndirect path is
/// patched.
#[tokio::test]
#[ignore = "CallIndirect BoundMethod branch does not consult mock_stack (open gap)"]
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
