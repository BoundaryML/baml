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

/// Recursion guard: a replacement that invokes its own target raises a
/// `baml.panics.UserPanic` instead of looping forever. The error is a
/// panic (not a regular throw), so a `catch (e) { baml.panics.UserPanic => … }`
/// pattern is needed to recover from it — `catch_all` alone won't.
#[tokio::test]
async fn mock_replacement_calling_target_throws_recursion_error() {
    let output = baml_test!(
        r#"
        function fetch_user(id: string) -> string { "real:" + id }
        function call_fetch(id: string) -> string { fetch_user(id) }

        function main() -> string {
            let m = baml.mock.Mock.new(fetch_user, (id: string) -> string {
                // BUG: replacement re-calls its own target instead of using
                // an explicit bypass primitive. Should panic, not loop.
                fetch_user(id)
            })
            let result = m.scope<string, unknown>(() -> string throws unknown {
                call_fetch("42")
            }) catch (e) {
                baml.panics.UserPanic => "recursion-caught",
            }
            result
        }
    "#
    );

    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("recursion-caught".to_string()))
    );
}

/// Once the replacement returns normally, the entry is un-suppressed and
/// subsequent calls inside the same scope still intercept (counter still
/// ticks). The guard only blocks re-entry *during* the replacement, not
/// for the rest of the scope.
#[tokio::test]
async fn mock_recursion_guard_unsuppresses_after_replacement_returns() {
    let output = baml_test!(
        r#"
        function double(n: int) -> int { n * 2 }
        function triple(n: int) -> int { n * 3 }
        function call_double(n: int) -> int { double(n) }

        function main() -> int {
            let m = baml.mock.Mock.new(double, triple)
            let sum = m.scope<int, never>(() -> int throws never {
                call_double(1) + call_double(2) + call_double(3)
            })
            // All three calls are intercepted (no recursion) and the
            // counter reflects them.
            sum * 100 + m.call_count
        }
    "#
    );

    // sum = 3 + 6 + 9 = 18 ; counter = 3 ; result = 1803
    assert_eq!(output.result, Ok(BexExternalValue::Int(1803)));
}

/// Nested scopes with overlapping targets: an inner replacement that
/// calls the target falls through to the outer mock's replacement (not
/// to the real target, and not as a recursion error). The guard only
/// fires when every matching entry is suppressed.
#[tokio::test]
async fn mock_recursion_guard_falls_through_to_outer_entry() {
    let output = baml_test!(
        r#"
        function double(n: int) -> int { n * 2 }
        function call_double(n: int) -> int { double(n) }

        function main() -> int {
            let outer = baml.mock.Mock.new(double, (n: int) -> int { 100 })
            let inner = baml.mock.Mock.new(double, (n: int) -> int {
                // The inner replacement calls `double` directly. With the
                // outer mock still pushed (and not suppressed), the
                // dispatch should land in the outer replacement.
                call_double(n)
            })
            outer.scope<int, never>(() -> int throws never {
                inner.scope<int, never>(() -> int throws never {
                    call_double(0)
                })
            })
        }
    "#
    );

    // call_double(0)
    //  → inner's replacement runs (inner.call_count = 1, suppressed=true)
    //     → call_double(0) inside it
    //        → inner is suppressed → falls through to outer's replacement
    //           → returns 100 (outer.call_count = 1)
    //     → returns 100
    //  → returns 100
    assert_eq!(output.result, Ok(BexExternalValue::Int(100)));
}

/// Per-instance counter: a `Mock.new(c1.bump, …)` only ticks `call_count`
/// for calls that resolve to `c1` — `c2.bump(…)` runs the original and
/// leaves the counter alone.
#[tokio::test]
async fn mock_per_instance_counter_only_counts_matching_receiver() {
    let output = baml_test!(
        r#"
        class Counter {
          function bump(self, n: int) -> int { n + 1 }
        }

        function main() -> int {
            let c1 = Counter {}
            let c2 = Counter {}
            let m = baml.mock.Mock.new(c1.bump, (n: int) -> int { 99 })
            let sum = m.scope<int, never>(() -> int throws never {
                c1.bump(0) + c2.bump(0) + c1.bump(0)
            })
            // sum = 99 + 1 + 99 = 199 ; call_count = 2 (only c1 ticks)
            sum * 100 + m.call_count
        }
    "#
    );

    assert_eq!(output.result, Ok(BexExternalValue::Int(19_902)));
}

/// Class-wide counter: a `Mock.new(Counter.bump, …)` ticks `call_count`
/// for every instance's call to that method.
#[tokio::test]
async fn mock_class_wide_counter_counts_every_instance() {
    let output = baml_test!(
        r#"
        class Counter {
          function bump(self, n: int) -> int { n + 1 }
        }

        function main() -> int {
            let c1 = Counter {}
            let c2 = Counter {}
            let m = baml.mock.Mock.new(Counter.bump, (self: Counter, n: int) -> int { 99 })
            let sum = m.scope<int, never>(() -> int throws never {
                c1.bump(0) + c2.bump(0) + c1.bump(0)
            })
            // sum = 99 + 99 + 99 = 297 ; call_count = 3
            sum * 100 + m.call_count
        }
    "#
    );

    assert_eq!(output.result, Ok(BexExternalValue::Int(29_703)));
}

/// SysOp interception (`baml.http.fetch`) still ticks `call_count` —
/// confirms the `DispatchFuture` + `WrapReadyFutureContinuation` path
/// hits the same counter wiring as plain Calls.
#[tokio::test]
async fn mock_sysop_intercept_bumps_call_count() {
    let output = baml_test!(
        r#"
        function main() -> int throws unknown {
            let fake = (url: string) -> baml.http.Response throws baml.errors.Io | baml.errors.Timeout {
                baml.http.Response {
                    status_code: 200,
                    headers: {},
                    url: url,
                    _body: "fake",
                }
            }
            let m = baml.mock.Mock.new(baml.http.fetch, fake)
            let _ = m.scope<int, unknown>(() -> int throws unknown {
                let _ = baml.http.fetch("https://a.example.invalid/")
                let _ = baml.http.fetch("https://b.example.invalid/")
                let _ = baml.http.fetch("https://c.example.invalid/")
                0
            })
            m.call_count
        }
    "#
    );

    assert_eq!(output.result, Ok(BexExternalValue::Int(3)));
}

/// `YieldToCall` interception (`array.map(target)`) ticks `call_count`
/// once per element — the native callback dispatch path bumps the
/// counter just like an `Instruction::Call` would.
#[tokio::test]
async fn mock_yield_to_call_intercept_bumps_call_count() {
    let output = baml_test!(
        r#"
        function double(n: int) -> int { n * 2 }
        function triple(n: int) -> int { n * 3 }

        function main() -> int {
            let m = baml.mock.Mock.new(double, triple)
            let _ = m.scope<int, unknown>(() -> int throws unknown {
                [1, 2, 3, 4, 5].map(double)
                    .reduce((acc: int, x: int) -> int { acc + x }, 0)
            })
            m.call_count
        }
    "#
    );

    assert_eq!(output.result, Ok(BexExternalValue::Int(5)));
}

/// If the body throws partway through, `call_count` still reflects the
/// interceptions that *did* happen before the throw — the counter is
/// updated eagerly per-call and is not rolled back on unwind.
#[tokio::test]
async fn mock_call_count_preserved_when_body_throws() {
    let output = baml_test!(
        r#"
        function double(n: int) -> int { n * 2 }
        function triple(n: int) -> int { n * 3 }
        function call_double(n: int) -> int { double(n) }

        function main() -> int {
            let m = baml.mock.Mock.new(double, triple)
            let _ = {
                m.scope<int, unknown>(() -> int throws unknown {
                    let _ = call_double(1)
                    let _ = call_double(2)
                    throw "boom"
                })
            } catch_all (e) {
                _ => 0,
            }
            // Two calls landed before the throw — counter ticks twice.
            m.call_count
        }
    "#
    );

    assert_eq!(output.result, Ok(BexExternalValue::Int(2)));
}

/// Nested scopes with two different mocks each track their own counter.
/// The outer mock's counter only ticks for calls inside the outer scope
/// that *also* dispatch to its target; the inner mock's counter ticks
/// only for its own intercepts.
#[tokio::test]
async fn mock_nested_scopes_track_independent_counters() {
    let output = baml_test!(
        r#"
        function alpha(n: int) -> int { n + 1 }
        function beta(n: int)  -> int { n + 2 }
        function call_alpha(n: int) -> int { alpha(n) }
        function call_beta(n: int)  -> int { beta(n) }

        function main() -> int {
            let m_alpha = baml.mock.Mock.new(alpha, (n: int) -> int { 100 })
            let m_beta  = baml.mock.Mock.new(beta,  (n: int) -> int { 200 })

            let _ = m_alpha.scope<int, never>(() -> int throws never {
                let _ = call_alpha(0)  // m_alpha tick
                let _ = m_beta.scope<int, never>(() -> int throws never {
                    let _ = call_alpha(0)  // m_alpha tick (still on stack)
                    let _ = call_beta(0)   // m_beta tick
                    let _ = call_beta(0)   // m_beta tick
                    0
                })
                let _ = call_alpha(0)  // m_alpha tick (m_beta popped)
                let _ = call_beta(0)   // no tick anywhere (m_beta popped, m_alpha doesn't match)
                0
            })

            // alpha intercepted: 3 times. beta intercepted: 2 times.
            m_alpha.call_count * 10 + m_beta.call_count
        }
    "#
    );

    // m_alpha = 3, m_beta = 2 → 32
    assert_eq!(output.result, Ok(BexExternalValue::Int(32)));
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
