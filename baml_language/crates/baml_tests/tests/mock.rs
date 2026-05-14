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
            let m = baml.mock.Mock.new(double);
            m.replace(triple);
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
            let m = baml.mock.Mock.new(add);
            m.replace(multiply);
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
            let m = baml.mock.Mock.new(baml.http.fetch);
            m.replace(fake);
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
            let m = baml.mock.Mock.new(original);
            m.replace((n: int) -> int { n + 100 });
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
            let m = baml.mock.Mock.new(double)
            m.replace(triple)
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
/// `baml.panics.UserPanic` instead of looping forever. The escape hatch
/// for the legitimate wrap-and-delegate case is `m.original()`; see
/// `mock_original_bypasses_active_override`.
#[tokio::test]
async fn mock_replacement_calling_target_throws_recursion_error() {
    let output = baml_test!(
        r#"
        function fetch_user(id: string) -> string { "real:" + id }
        function call_fetch(id: string) -> string { fetch_user(id) }

        function main() -> string {
            let m = baml.mock.Mock.new(fetch_user)
            m.replace((id: string) -> string {
                // BUG: replacement re-calls its own target directly instead
                // of going through `m.original()`. Should panic, not loop.
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
/// ticks). The guard only blocks re-entry *during* the replacement.
#[tokio::test]
async fn mock_recursion_guard_unsuppresses_after_replacement_returns() {
    let output = baml_test!(
        r#"
        function double(n: int) -> int { n * 2 }
        function triple(n: int) -> int { n * 3 }
        function call_double(n: int) -> int { double(n) }

        function main() -> int {
            let m = baml.mock.Mock.new(double)
            m.replace(triple)
            let sum = m.scope<int, never>(() -> int throws never {
                call_double(1) + call_double(2) + call_double(3)
            })
            sum * 100 + m.call_count
        }
    "#
    );

    // sum = 3 + 6 + 9 = 18 ; counter = 3 ; result = 1803
    assert_eq!(output.result, Ok(BexExternalValue::Int(1803)));
}

/// Nested scopes with overlapping targets: an inner replacement that calls
/// the target falls through to the outer mock's replacement.
#[tokio::test]
async fn mock_recursion_guard_falls_through_to_outer_entry() {
    let output = baml_test!(
        r#"
        function double(n: int) -> int { n * 2 }
        function call_double(n: int) -> int { double(n) }

        function main() -> int {
            let outer = baml.mock.Mock.new(double)
            outer.replace((n: int) -> int { 100 })
            let inner = baml.mock.Mock.new(double)
            inner.replace((n: int) -> int {
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

    assert_eq!(output.result, Ok(BexExternalValue::Int(100)));
}

/// `m.original()` returns a callable that dispatches the un-mocked target
/// even while `m` is active inside `scope`. The wrap-and-delegate pattern.
#[tokio::test]
async fn mock_original_bypasses_active_override() {
    let output = baml_test!(
        r#"
        function double(n: int) -> int { n * 2 }
        function call_double(n: int) -> int { double(n) }

        function main() -> int {
            let m = baml.mock.Mock.new(double)
            // Replacement adds 1000 then delegates to the real `double`.
            m.replace((n: int) -> int {
                m.original(n) + 1000
            })
            let r = m.scope<int, never>(() -> int throws never {
                call_double(5)
            })
            // call_double(5) → intercepted → replacement runs
            //   m.original()(5) → real double(5) = 10
            //   10 + 1000 = 1010
            r * 10 + m.call_count
        }
    "#
    );

    // r = 1010 ; call_count = 1 ; result = 10101
    assert_eq!(output.result, Ok(BexExternalValue::Int(10_101)));
}

/// Spy mode: `Mock.new(f)` without `replace` leaves `_impl = null`, so
/// the override is a *spy* entry — matching calls bump `call_count` and
/// fall through to `f` un-intercepted (no replacement frame is ever
/// pushed). Natural recursion in `f` works fine because the recursion
/// guard's suppression flag is only set when a real replacement is
/// dispatched.
#[tokio::test]
async fn mock_spy_passes_through_and_supports_recursion() {
    let output = baml_test!(
        r#"
        // Classic recursive function — fib(5) = 5.
        function fib(n: int) -> int {
            if (n < 2) { n } else { fib(n - 1) + fib(n - 2) }
        }
        function call_fib(n: int) -> int { fib(n) }

        function main() -> int {
            let m = baml.mock.Mock.new(fib)
            // No replace — pure spy.
            let r = m.scope<int, never>(() -> int throws never {
                call_fib(5)
            })
            // fib(5) = 5; call_count should be 15 (the count of fib calls
            // made during the computation of fib(5)).
            // fib(5) calls: fib(5), fib(4), fib(3), fib(2), fib(1), fib(0),
            // fib(1), fib(2), fib(1), fib(0), fib(3), fib(2), fib(1), fib(0),
            // fib(1) = 15 total invocations.
            r * 100 + m.call_count
        }
    "#
    );

    // r = 5, call_count = 15 → 515
    assert_eq!(output.result, Ok(BexExternalValue::Int(515)));
}

/// Per-instance counter: `Mock.new(c1.bump)` only ticks for calls that
/// resolve to `c1`.
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
            let m = baml.mock.Mock.new(c1.bump)
            m.replace((n: int) -> int { 99 })
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

/// Class-wide counter: `Mock.new(Counter.bump)` ticks for every instance's
/// call to that method.
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
            let m = baml.mock.Mock.new(Counter.bump)
            m.replace((self: Counter, n: int) -> int { 99 })
            let sum = m.scope<int, never>(() -> int throws never {
                c1.bump(0) + c2.bump(0) + c1.bump(0)
            })
            sum * 100 + m.call_count
        }
    "#
    );

    assert_eq!(output.result, Ok(BexExternalValue::Int(29_703)));
}

/// SysOp interception ticks `call_count`.
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
            let m = baml.mock.Mock.new(baml.http.fetch)
            m.replace(fake)
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

/// `YieldToCall` interception (`array.map(target)`) ticks `call_count`.
#[tokio::test]
async fn mock_yield_to_call_intercept_bumps_call_count() {
    let output = baml_test!(
        r#"
        function double(n: int) -> int { n * 2 }
        function triple(n: int) -> int { n * 3 }

        function main() -> int {
            let m = baml.mock.Mock.new(double)
            m.replace(triple)
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
/// interceptions that landed before the throw.
#[tokio::test]
async fn mock_call_count_preserved_when_body_throws() {
    let output = baml_test!(
        r#"
        function double(n: int) -> int { n * 2 }
        function triple(n: int) -> int { n * 3 }
        function call_double(n: int) -> int { double(n) }

        function main() -> int {
            let m = baml.mock.Mock.new(double)
            m.replace(triple)
            let _ = {
                m.scope<int, unknown>(() -> int throws unknown {
                    let _ = call_double(1)
                    let _ = call_double(2)
                    throw "boom"
                })
            } catch_all (e) {
                _ => 0,
            }
            m.call_count
        }
    "#
    );

    assert_eq!(output.result, Ok(BexExternalValue::Int(2)));
}

/// Nested scopes with two different mocks each track their own counter.
#[tokio::test]
async fn mock_nested_scopes_track_independent_counters() {
    let output = baml_test!(
        r#"
        function alpha(n: int) -> int { n + 1 }
        function beta(n: int)  -> int { n + 2 }
        function call_alpha(n: int) -> int { alpha(n) }
        function call_beta(n: int)  -> int { beta(n) }

        function main() -> int {
            let m_alpha = baml.mock.Mock.new(alpha)
            m_alpha.replace((n: int) -> int { 100 })
            let m_beta  = baml.mock.Mock.new(beta)
            m_beta.replace((n: int) -> int { 200 })

            let _ = m_alpha.scope<int, never>(() -> int throws never {
                let _ = call_alpha(0)
                let _ = m_beta.scope<int, never>(() -> int throws never {
                    let _ = call_alpha(0)
                    let _ = call_beta(0)
                    let _ = call_beta(0)
                    0
                })
                let _ = call_alpha(0)
                let _ = call_beta(0)
                0
            })

            m_alpha.call_count * 10 + m_beta.call_count
        }
    "#
    );

    // alpha intercepted 3 times, beta intercepted 2 times → 32
    assert_eq!(output.result, Ok(BexExternalValue::Int(32)));
}

/// A direct method call (`c.method(arg)`) is intercepted.
#[tokio::test]
async fn mock_intercepts_direct_method_call() {
    let output = baml_test!(
        r#"
        class Counter {
          function bump(self, n: int) -> int { n + 1 }
        }

        function main() -> int {
            let c = Counter {}
            let m = baml.mock.Mock.new(c.bump)
            m.replace((n: int) -> int { 99 })
            m.scope<int, never>(() -> int throws never { c.bump(10) })
        }
    "#
    );

    assert_eq!(output.result, Ok(BexExternalValue::Int(99)));
}

/// `Mock.new(c1.bump)` is per-instance.
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
            let m = baml.mock.Mock.new(c1.bump)
            m.replace((n: int) -> int { 99 })
            m.scope<int, never>(() -> int throws never {
                c1.bump(10) + c2.bump(10)
            })
        }
    "#
    );

    // c1.bump intercepted (99); c2.bump original (11). Sum = 110.
    assert_eq!(output.result, Ok(BexExternalValue::Int(110)));
}

/// `Mock.new(Counter.bump)` is class-wide.
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
            let m = baml.mock.Mock.new(Counter.bump)
            m.replace((self: Counter, n: int) -> int { 99 })
            m.scope<int, never>(() -> int throws never {
                c1.bump(10) + c2.bump(10)
            })
        }
    "#
    );

    assert_eq!(output.result, Ok(BexExternalValue::Int(198)));
}

/// `CallIndirect` BoundMethod branch consults `mock_stack`.
#[tokio::test]
async fn mock_intercepts_indirect_bound_method_call() {
    let output = baml_test!(
        r#"
        class Counter {
          function bump(self, n: int) -> int { n + 1 }
        }

        function main() -> int {
            let c = Counter {}
            let m = baml.mock.Mock.new(c.bump)
            m.replace((n: int) -> int { 99 })
            m.scope<int, never>(() -> int throws never {
                let bound = c.bump;
                bound(10)
            })
        }
    "#
    );

    assert_eq!(output.result, Ok(BexExternalValue::Int(99)));
}

/// `array.map(target)` callbacks dispatched via `YieldToCall` route
/// through the mock.
#[tokio::test]
async fn mock_intercepts_yield_to_call_callback() {
    let output = baml_test!(
        r#"
        function double(n: int) -> int { n * 2 }
        function triple(n: int) -> int { n * 3 }

        function main() -> int {
            let m = baml.mock.Mock.new(double)
            m.replace(triple)
            m.scope<int, unknown>(() -> int throws unknown {
                [1, 2, 3, 4, 5].map(double)
                    .reduce((acc: int, x: int) -> int { acc + x }, 0)
            })
        }
    "#
    );

    // Triple applied to [1..=5] sums to 3+6+9+12+15 = 45.
    assert_eq!(output.result, Ok(BexExternalValue::Int(45)));
}

/// GC stress: a Closure replacement that captures a heap-allocated cell
/// is pushed onto the mock stack, and the body allocates aggressively to
/// force collection while the override is active.
#[tokio::test]
async fn mock_scope_survives_gc_under_alloc_pressure() {
    let output = baml_test!(
        r#"
        function target(n: int) -> int { n + 1 }
        function call_target(n: int) -> int { target(n) }

        function main() -> int {
            let counter = 0;
            let replacement = (n: int) -> int {
                counter = counter + 1;
                n * 100
            };
            let m = baml.mock.Mock.new(target);
            m.replace(replacement);

            let sum = m.scope<int, unknown>(() -> int throws unknown {
                let acc = 0;
                let i = 0;
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

            sum + counter
        }
        "#
    );

    // Replacement does `n * 100` for n = 0..5000.
    // sum_{i=0}^{4999} i*100 = 1_249_750_000 ; counter = 5000 ; total = 1_249_755_000
    assert_eq!(output.result, Ok(BexExternalValue::Int(1_249_755_000)));
}

/// Mocking a generic target invoked with explicit type args at the call
/// site: the redirect should still fire, and the (monomorphic)
/// replacement should run with the supplied arg regardless of the type
/// args passed to the original.
#[tokio::test]
async fn mock_intercepts_generic_target_with_type_args() {
    let output = baml_test!(
        r#"
        function identity<T>(x: T) -> T { x }
        function call_int(n: int) -> int { identity<int>(n) }

        function main() -> int {
            let m = baml.mock.Mock.new(identity)
            // Monomorphic replacement — typed (int) -> int even though the
            // target is generic over T.
            m.replace((x: int) -> int { x * 100 })
            let inside = m.scope<int, never>(() -> int throws never {
                call_int(5)
            })
            let outside = call_int(7)
            inside * 1000 + outside * 10 + m.call_count
        }
    "#
    );

    // inside  = replacement(5) = 500
    // outside = identity(7)    = 7
    // counter                  = 1
    // result  = 500_000 + 70 + 1 = 500_071
    assert_eq!(output.result, Ok(BexExternalValue::Int(500_071)));
}

/// Two mocks on the same target, nested scopes: the inner mock shadows
/// the outer one. Calls inside the inner scope hit the inner replacement;
/// calls in the outer-but-not-inner region hit the outer replacement.
#[tokio::test]
async fn mock_nested_same_target_inner_shadows_outer() {
    let output = baml_test!(
        r#"
        function fetch(id: string) -> string { "real:" + id }
        function call_fetch(id: string) -> string { fetch(id) }

        function main() -> string {
            let outer = baml.mock.Mock.new(fetch)
            outer.replace((id: string) -> string { "outer:" + id })
            let inner = baml.mock.Mock.new(fetch)
            inner.replace((id: string) -> string { "inner:" + id })

            let log = []

            let _ = outer.scope<int, never>(() -> int throws never {
                let _ = log.push(call_fetch("a"))   // outer → "outer:a"
                let _ = inner.scope<int, never>(() -> int throws never {
                    let _ = log.push(call_fetch("b"))   // inner → "inner:b"
                    0
                })
                let _ = log.push(call_fetch("c"))   // back to outer → "outer:c"
                0
            })
            let _ = log.push(call_fetch("d"))         // no mock → "real:d"

            log.join("|")
        }
    "#
    );

    assert_eq!(
        output.result,
        Ok(BexExternalValue::String(
            "outer:a|inner:b|outer:c|real:d".to_string()
        ))
    );
}

/// Two mocks on the same target with independent `call_count` fields:
/// the outer counter only ticks when its scope is the topmost match;
/// the inner ticks for calls inside its own scope.
#[tokio::test]
async fn mock_nested_same_target_independent_counters() {
    let output = baml_test!(
        r#"
        function fetch(id: string) -> string { "real:" + id }
        function call_fetch(id: string) -> string { fetch(id) }

        function main() -> int {
            let outer = baml.mock.Mock.new(fetch)
            outer.replace((id: string) -> string { "outer" })
            let inner = baml.mock.Mock.new(fetch)
            inner.replace((id: string) -> string { "inner" })

            let _ = outer.scope<int, never>(() -> int throws never {
                let _ = call_fetch("1")          // outer.call_count = 1
                let _ = call_fetch("2")          // outer.call_count = 2
                let _ = inner.scope<int, never>(() -> int throws never {
                    let _ = call_fetch("3")      // inner.call_count = 1
                    let _ = call_fetch("4")      // inner.call_count = 2
                    let _ = call_fetch("5")      // inner.call_count = 3
                    0
                })
                let _ = call_fetch("6")          // outer.call_count = 3
                0
            })
            outer.call_count * 100 + inner.call_count
        }
    "#
    );

    // outer = 3, inner = 3 → 303
    assert_eq!(output.result, Ok(BexExternalValue::Int(303)));
}

/// `m.original` bypasses the **entire** active mock stack — not just
/// the entry's own override. Even when an outer mock for the same
/// target is still pushed, `inner.original(x)` dispatches the raw,
/// un-mocked `fetch`. Matches Python `unittest.mock`'s `wraps=`
/// semantics: "original" means original, full stop.
#[tokio::test]
async fn mock_original_bypasses_entire_mock_stack() {
    let output = baml_test!(
        r#"
        function fetch(id: string) -> string { "real:" + id }
        function call_fetch(id: string) -> string { fetch(id) }

        function main() -> string {
            let outer = baml.mock.Mock.new(fetch)
            outer.replace((id: string) -> string { "outer:" + id })

            let inner = baml.mock.Mock.new(fetch)
            inner.replace((id: string) -> string {
                // `inner.original` is an UnmockedRef whose CallIndirect
                // path skips the mock-stack lookup entirely — even though
                // outer's entry is still on the stack, the dispatch lands
                // in the real `fetch`.
                "inner-wraps[" + inner.original(id) + "]"
            })

            outer.scope<string, never>(() -> string throws never {
                inner.scope<string, never>(() -> string throws never {
                    call_fetch("x")
                })
            })
        }
    "#
    );

    // call_fetch("x") → inner intercepts → inner-wraps[ inner.original("x") ]
    // inner.original("x") bypasses ALL mocks → real fetch → "real:x"
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("inner-wraps[real:x]".to_string()))
    );
}

/// Mocking a closure target with captures. The closure is stored in a
/// local, captured by the call-site context, and mocked. Inside the
/// scope, calls through the same local should be intercepted; the
/// captured state is irrelevant to the mock identity (it's the inner
/// Function ptr that matters, via `callable_identity`).
#[tokio::test]
async fn mock_intercepts_closure_target_with_captures() {
    let output = baml_test!(
        r#"
        function call_f(f: (int) -> int, x: int) -> int { f(x) }

        function main() -> int {
            let captured = 7
            // Closure that captures `captured`.
            let f = (x: int) -> int { x + captured }

            let m = baml.mock.Mock.new(f)
            m.replace((x: int) -> int { x * 100 })

            let inside = m.scope<int, never>(() -> int throws never {
                call_f(f, 5)               // intercepted → replacement(5) = 500
            })
            let outside = call_f(f, 5)     // real closure → 5 + 7 = 12

            inside * 100 + outside * 10 + m.call_count
        }
    "#
    );

    // inside = 500, outside = 12, count = 1 → 50_121
    assert_eq!(output.result, Ok(BexExternalValue::Int(50_121)));
}

/// `m.original` for a SysOp target. The target is `baml.sys.sleep` (an
/// async SysOp). `CallIndirect` of an `UnmockedRef` wrapping a SysOp
/// yields `DispatchSysOpInline`, which the engine handles by scheduling
/// + awaiting in one round-trip and pushing the resolved value back —
/// so wrap-and-delegate works for async targets just like sync ones.
#[tokio::test]
async fn mock_original_works_on_sysop_target() {
    let output = baml_test!(
        r#"
        function main() -> int throws unknown {
            let m = baml.mock.Mock.new(baml.sys.sleep)
            // Replacement wraps the real sleep. If m.original for a
            // SysOp didn't route through the engine yield path, the
            // CallIndirect inside would crash with "SysOps must go
            // through the engine yield path".
            m.replace((ms: int) -> null throws baml.errors.Io {
                m.original(ms)
            })
            m.scope<int, unknown>(() -> int throws unknown {
                let _ = baml.sys.sleep(0)
                let _ = baml.sys.sleep(0)
                let _ = baml.sys.sleep(0)
                m.call_count
            })
        }
    "#
    );

    // Three intercepted sleeps; each delegated through m.original to
    // the real (async, no-op) sleep. Counter = 3.
    assert_eq!(output.result, Ok(BexExternalValue::Int(3)));
}

/// Mock chain: `a` replaced by named function `b`, `b` replaced by `c`.
/// Confirms what does and doesn't chain:
///
/// - `call_a(5)`: `m_a` intercepts (by-name call to `a`) and dispatches
///   `b` directly. `b`'s body runs as-is. `m_b` does NOT fire because
///   nothing inside `b`'s body calls `b` by name — it's just `x + 100`.
/// - `call_b(5)`: `m_b` intercepts (by-name call to `b`) and dispatches
///   `c`. Returns `1005`.
///
/// Mocks intercept *named call sites*, not internal dispatch. A
/// replacement that's "just another function" doesn't transitively
/// re-enter the mock stack unless its own body makes a named call to
/// the further-mocked target.
#[tokio::test]
async fn mock_chained_named_function_replacements() {
    let output = baml_test!(
        r#"
        function a(x: int) -> int { x + 1 }
        function b(x: int) -> int { x + 100 }
        function c(x: int) -> int { x + 1000 }
        function call_a(x: int) -> int { a(x) }
        function call_b(x: int) -> int { b(x) }

        function main() -> int {
            let m_a = baml.mock.Mock.new(a)
            m_a.replace(b)
            let m_b = baml.mock.Mock.new(b)
            m_b.replace(c)

            m_a.scope<int, never>(() -> int throws never {
                m_b.scope<int, never>(() -> int throws never {
                    let from_a = call_a(5)   // a → b   → 105 (m_b dormant)
                    let from_b = call_b(5)   // b → c   → 1005
                    // Encode results + counters into a single int.
                    from_a * 10000000
                        + from_b * 1000
                        + m_a.call_count * 10
                        + m_b.call_count
                })
            })
        }
    "#
    );

    // from_a = 105, from_b = 1005, m_a.call_count = 1, m_b.call_count = 1
    // 105 * 10_000_000 + 1005 * 1000 + 1 * 10 + 1 = 1_051_005_011
    assert_eq!(output.result, Ok(BexExternalValue::Int(1_051_005_011)));
}

/// Mock chain where the inner mock *does* fire: `b` (mocked → `c`)
/// recurses, so the recursive call `b(x-1)` IS a named call to `b` and
/// hits `m_b`. Confirms that whether chaining happens depends on whether
/// the replacement's body makes named calls to further-mocked targets.
#[tokio::test]
async fn mock_chained_replacements_fire_on_internal_named_calls() {
    let output = baml_test!(
        r#"
        function a(x: int) -> int { x + 1 }
        // b recurses by name — so the recursive call to b will hit m_b.
        function b(x: int) -> int {
            if (x > 0) { b(x - 1) } else { 9999 }
        }
        function c(x: int) -> int { x + 1000 }
        function call_a(x: int) -> int { a(x) }

        function main() -> int {
            let m_a = baml.mock.Mock.new(a)
            m_a.replace(b)
            let m_b = baml.mock.Mock.new(b)
            m_b.replace(c)

            m_a.scope<int, never>(() -> int throws never {
                m_b.scope<int, never>(() -> int throws never {
                    let r = call_a(2)
                    // a(2) → m_a → dispatch b → b(2) body executes
                    //   b body: x=2 > 0 → call b(1) by name
                    //     m_b fires → dispatch c → c(1) = 1001
                    //   b's body returns 1001
                    // m_a.call_count = 1, m_b.call_count = 1
                    r * 100 + m_a.call_count * 10 + m_b.call_count
                })
            })
        }
    "#
    );

    // r = 1001, m_a.count = 1, m_b.count = 1 → 100_111
    assert_eq!(output.result, Ok(BexExternalValue::Int(100_111)));
}

/// Circular mocking: `a` is redirected to `b`, and `b` internally calls
/// `a`. Chain: `a(5) → m_a fires → dispatch b → b body calls a → a's
/// override is still suppressed → recursion guard panics`. Without the
/// guard this would infinite-loop.
#[tokio::test]
async fn mock_circular_dispatch_caught_by_recursion_guard() {
    let output = baml_test!(
        r#"
        function a(x: int) -> int { x + 1 }
        function b(x: int) -> int { a(x) + 100 }   // b calls a — closes the loop
        function call_a(x: int) -> int { a(x) }

        function main() -> string {
            let m_a = baml.mock.Mock.new(a)
            m_a.replace(b)
            let m_b = baml.mock.Mock.new(b)
            m_b.replace(a)

            m_a.scope<string, unknown>(() -> string throws unknown {
                m_b.scope<string, unknown>(() -> string throws unknown {
                    // a(5) → m_a → dispatch b → b body invokes a
                    //   → m_a is suppressed → only-suppressed-match → panic.
                    let v = call_a(5)
                    "got " + baml.unstable.string(v)
                })
            }) catch (e) {
                baml.panics.UserPanic => "cycle-detected",
            }
        }
    "#
    );

    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("cycle-detected".to_string()))
    );
}

/// Same circular setup, but `b`'s call-back-to-`a` is routed through
/// `m_a.original` — which bypasses the entire mock stack and dispatches
/// the real `a`. The cycle terminates and the test returns a value.
#[tokio::test]
async fn mock_circular_dispatch_resolves_via_original_bypass() {
    let output = baml_test!(
        r#"
        function a(x: int) -> int { x + 1 }
        function call_a(x: int) -> int { a(x) }

        function main() -> int {
            let m_a = baml.mock.Mock.new(a)
            // `b` is defined inline as a closure that uses m_a.original to
            // dodge the active m_a override when it re-invokes a.
            let b = (x: int) -> int { m_a.original(x) + 100 }
            m_a.replace(b)

            m_a.scope<int, never>(() -> int throws never {
                // a(5) → m_a fires → dispatch b → b runs m_a.original(5) →
                //   real a(5) = 6 → + 100 = 106
                call_a(5)
            })
        }
    "#
    );

    assert_eq!(output.result, Ok(BexExternalValue::Int(106)));
}

/// Mocking a generic target with a *generic* replacement: the type args
/// passed at the call site (`identity<int>(7)`) must flow into the
/// replacement frame so that `reflect.type_of<T>()` inside resolves to
/// the same type the caller used. The replacement explicitly panics if
/// `T` doesn't match, so a silent "type args didn't propagate" failure
/// surfaces as an error result instead of a passing 7.
#[tokio::test]
async fn mock_intercepts_generic_target_with_generic_replacement() {
    let output = baml_test!(
        r#"
        function identity<T>(x: T) -> T { x }

        // Generic replacement — observes `T` at runtime via
        // reflect.type_of and panics if it isn't the expected `int`.
        // If type args didn't flow into the replacement frame, the
        // panic fires and the test fails with an Err result.
        function strict_spy<T>(x: T) -> T {
            let t_str = baml.unstable.string(reflect.type_of<T>())
            if (t_str != "<type: int>") {
                baml.sys.panic("expected T=int inside replacement but got: " + t_str)
            } else {
                null
            }
            x
        }

        function call_int() -> int { identity<int>(7) }

        function main() -> int {
            let m = baml.mock.Mock.new(identity)
            m.replace(strict_spy)
            let r = m.scope<int, never>(() -> int throws never {
                call_int()
            })
            r * 100 + m.call_count
        }
    "#
    );

    // strict_spy<int>(7) = 7 ; call_count = 1 → 701
    // If type args didn't propagate, strict_spy would panic with a
    // mismatched-T message and `output.result` would be `Err(...)`.
    assert_eq!(output.result, Ok(BexExternalValue::Int(701)));
}

/// Multiple `m.replace()` calls in sequence: the latest one wins.
/// `_impl` is overwritten each time; `scope` uses whatever is current at
/// scope-entry. There's no history or stacking.
#[tokio::test]
async fn mock_replace_called_repeatedly_uses_latest() {
    let output = baml_test!(
        r#"
        function target(n: int) -> int { n + 1 }
        function call_target(n: int) -> int { target(n) }

        function main() -> int {
            let m = baml.mock.Mock.new(target)
            m.replace((n: int) -> int { n + 10 })
            m.replace((n: int) -> int { n + 100 })
            m.replace((n: int) -> int { n + 1000 })
            m.scope<int, never>(() -> int throws never { call_target(5) })
        }
    "#
    );

    // Latest replace wins: 5 + 1000 = 1005
    assert_eq!(output.result, Ok(BexExternalValue::Int(1005)));
}

/// If the replacement itself throws (rather than the scope body), the
/// throw propagates through `scope` to the caller. The override entry
/// is still popped, `call_count` is incremented for the intercepted
/// call (the throw happened *inside* the replacement, so the count
/// reflects the dispatch), and subsequent calls after the catch see no
/// active override.
#[tokio::test]
async fn mock_replacement_throws_propagates_and_unwinds() {
    let output = baml_test!(
        r#"
        // Target declared as `throws unknown` so the replacement is
        // allowed to throw too.
        function target(n: int) -> int throws unknown { n + 1 }
        function call_target(n: int) -> int throws unknown { target(n) }

        function main() -> int {
            let m = baml.mock.Mock.new(target)
            m.replace((n: int) -> int throws unknown { throw "replacement-boom" })

            let caught = {
                m.scope<int, unknown>(() -> int throws unknown {
                    call_target(5)
                })
            } catch_all (e) {
                _ => -1,
            }

            let outside = {
                call_target(7)
            } catch_all (e) {
                _ => -2,
            }

            // caught=-1 → throw escaped the replacement and was caught here.
            // outside=8 → after unwind, target is un-mocked: 7+1.
            // call_count=1 → the one intercept that triggered the throw.
            caught * 100 + outside * 10 + m.call_count
        }
    "#
    );

    // caught = -1, outside = 8, call_count = 1 → -100 + 80 + 1 = -19
    assert_eq!(output.result, Ok(BexExternalValue::Int(-19)));
}

/// `m.original` is a regular field of type `T`. Calling it outside an
/// active scope still works — it's just an `UnmockedRef` wrapping the
/// original function, and since no override is on the stack, the
/// "bypass" semantics are a no-op. The result is identical to calling
/// the underlying function directly.
#[tokio::test]
async fn mock_original_works_outside_active_scope() {
    let output = baml_test!(
        r#"
        function target(n: int) -> int { n + 1 }

        function main() -> int {
            let m = baml.mock.Mock.new(target)
            m.replace((n: int) -> int { 999 })   // would fire if scope active

            // No scope around this — m.original just dispatches the
            // un-mocked target. Equivalent to calling `target` directly.
            let r1 = m.original(5)        // = target(5) = 6
            let r2 = m.original(10)       // = target(10) = 11

            // call_count should NOT tick — m.original bypasses the
            // intercept entirely; the mock isn't even on the stack.
            r1 * 1000 + r2 * 10 + m.call_count
        }
    "#
    );

    // r1 = 6, r2 = 11, count = 0 → 6000 + 110 + 0 = 6110
    assert_eq!(output.result, Ok(BexExternalValue::Int(6110)));
}

/// Mocking an LLM function. LLM functions are async and dispatched
/// through the same `DispatchFuture` path as other SysOps, so they
/// should intercept correctly. The mock replaces the LLM with a
/// deterministic function — letting tests verify code paths without
/// hitting a real model.
#[tokio::test]
async fn mock_intercepts_llm_function() {
    let output = baml_test!(
        r##"
        client<llm> TestClient {
            provider openai
            options {
                model "gpt-4o"
                api_key env.OPENAI_API_KEY
            }
        }

        function Classify(text: string) -> string {
            client TestClient
            prompt #"classify {{ text }}"#
        }

        function call_classify(text: string) -> string throws unknown {
            Classify(text)
        }

        function main() -> string throws unknown {
            let m = baml.mock.Mock.new(Classify)
            m.replace((text: string) -> string { "fake-positive:" + text })

            let inside = m.scope<string, unknown>(() -> string throws unknown {
                call_classify("hello world")
            })

            inside + "|count=" + baml.unstable.string(m.call_count)
        }
    "##
    );

    assert_eq!(
        output.result,
        Ok(BexExternalValue::String(
            "fake-positive:hello world|count=1".to_string()
        ))
    );
}

/// Sequential `m.scope(...)` calls on the same Mock instance. After the
/// first scope returns, its `mock_stack` entry is popped. A second
/// scope on the same Mock pushes a fresh entry; the counter persists
/// across both scopes (it's a field, not scope-local).
#[tokio::test]
async fn mock_sequential_scopes_share_call_count() {
    let output = baml_test!(
        r#"
        function target(n: int) -> int { n + 1 }
        function call_target(n: int) -> int { target(n) }

        function main() -> int {
            let m = baml.mock.Mock.new(target)
            m.replace((n: int) -> int { n + 100 })

            let r1 = m.scope<int, never>(() -> int throws never {
                call_target(1) + call_target(2)        // m fires twice
            })
            let r2 = m.scope<int, never>(() -> int throws never {
                call_target(3)                         // m fires once
            })
            let outside = call_target(4)               // un-mocked: 5

            // r1 = 101 + 102 = 203
            // r2 = 103
            // outside = 5
            // call_count = 3 (cumulative across both scopes)
            r1 * 10000 + r2 * 1000 + outside * 100 + m.call_count
        }
    "#
    );

    // 203 * 10_000 + 103 * 1000 + 5 * 100 + 3 = 2_133_503
    assert_eq!(output.result, Ok(BexExternalValue::Int(2_133_503)));
}

/// Re-entrant `m.scope`: body of outer `m.scope` calls `m.scope` on the
/// *same* mock. Two entries for the same target on the stack. When the
/// replacement is invoked from the inner body, lookup finds the inner
/// (suppressed during dispatch) and the outer (unsuppressed) — so a
/// recursive call from inside the replacement falls through to the
/// outer replacement, not a panic.
///
/// This documents the semantics: re-entrant `scope` on the same Mock
/// stacks identically to two-distinct-mocks-on-the-same-target.
#[tokio::test]
async fn mock_reentrant_same_scope_stacks_entries() {
    let output = baml_test!(
        r#"
        function target(n: int) -> int { n + 1 }
        function call_target(n: int) -> int { target(n) }

        function main() -> int {
            let m = baml.mock.Mock.new(target)
            m.replace((n: int) -> int { n + 100 })

            let r = m.scope<int, never>(() -> int throws never {
                let r_outer = call_target(5)           // 105 (outer m)
                let r_inner = m.scope<int, never>(() -> int throws never {
                    call_target(10)                    // 110 (inner m, same _impl)
                })
                r_outer * 1000 + r_inner
            })
            r * 10 + m.call_count
        }
    "#
    );

    // r_outer = 105, r_inner = 110, r = 105_110
    // call_count = 2 (one tick per intercept, regardless of which entry)
    // result = 1_051_100 + 2 = 1_051_102
    assert_eq!(output.result, Ok(BexExternalValue::Int(1_051_102)));
}

/// `m.replace` called mid-scope mutates `_impl` but does NOT update the
/// already-pushed `mock_stack` entry — the entry was captured at
/// scope-entry. Documents the "replace before scope" contract.
#[tokio::test]
async fn mock_replace_mid_scope_does_not_swap_active_entry() {
    let output = baml_test!(
        r#"
        function target(n: int) -> int { n + 1 }
        function call_target(n: int) -> int { target(n) }

        function main() -> int {
            let m = baml.mock.Mock.new(target)
            m.replace((n: int) -> int { n + 100 })       // installed before scope

            let r1 = m.scope<int, never>(() -> int throws never {
                let before = call_target(5)              // 105 (uses the +100 impl)
                // Replace mid-scope. The mock_stack entry was already
                // pushed; this only updates `_impl` on the instance, not
                // the entry's `replacement` ptr. So the in-flight call
                // still uses the original +100.
                m.replace((n: int) -> int { n + 1000 })
                let after = call_target(5)               // still 105
                before * 1000 + after
            })

            // Next scope sees the new impl.
            let r2 = m.scope<int, never>(() -> int throws never {
                call_target(5)                           // 1005
            })

            r1 * 1000000 + r2 * 100 + m.call_count
        }
    "#
    );

    // r1 = 105 * 1000 + 105 = 105_105
    // r2 = 1005
    // call_count = 3
    // result = 105_105 * 1_000_000 + 1005 * 100 + 3 = 105_105_100_503
    assert_eq!(output.result, Ok(BexExternalValue::Int(105_105_100_503)));
}

/// GC stress test specifically targeting `UnmockedRef`: keep `m.original`
/// alive and invoked repeatedly while allocating heavily, so the
/// UnmockedRef itself and its inner pointer must survive collection
/// cycles intact. If the GC tracing for UnmockedRef were missing,
/// `m.original` would dereference into freed memory and crash.
#[tokio::test]
async fn mock_unmocked_ref_survives_gc() {
    let output = baml_test!(
        r#"
        function target(n: int) -> int { n + 1 }
        function call_target(n: int) -> int { target(n) }

        function main() -> int {
            let m = baml.mock.Mock.new(target)
            m.replace((n: int) -> int {
                // Wrap-and-delegate: each intercept routes through
                // m.original. The UnmockedRef is read every iteration.
                m.original(n)
            })

            let sum = m.scope<int, unknown>(() -> int throws unknown {
                let acc = 0
                let i = 0
                while (i < 3000) {
                    let scratch = [
                        [i, i + 1, i + 2],
                        [i + 3, i + 4, i + 5],
                    ]
                    acc = acc + call_target(scratch[0][0])
                    if (i % 100 == 0) {
                        let _ = baml.sys.sleep(0)        // engine yield → GC opportunity
                    } else {
                        let _ = 0
                    }
                    i = i + 1
                }
                acc
            })

            sum + m.call_count
        }
    "#
    );

    // Each call: replacement runs → m.original(i) → real target(i) = i + 1.
    // sum_{i=0}^{2999} (i + 1) = (3000 * 3001 / 2) = 4_501_500.
    // call_count = 3000.
    // total = 4_501_500 + 3000 = 4_504_500.
    assert_eq!(output.result, Ok(BexExternalValue::Int(4_504_500)));
}

/// Mocking a function whose replacement captures *another* Mock. When
/// the outer mock's replacement runs, it makes calls that trip the
/// inner mock — confirming Mock instances compose cleanly via closure
/// capture.
#[tokio::test]
async fn mock_replacement_captures_another_mock() {
    let output = baml_test!(
        r#"
        function outer_target(n: int) -> int { n + 1 }
        function helper(n: int) -> int { n * 10 }
        function call_outer(n: int) -> int { outer_target(n) }

        function main() -> int {
            // Inner mock on `helper`.
            let m_helper = baml.mock.Mock.new(helper)
            m_helper.replace((n: int) -> int { 999 })

            // Outer mock on `outer_target` — its replacement *calls helper*,
            // which the inner mock will intercept iff its scope is active.
            let m_outer = baml.mock.Mock.new(outer_target)
            m_outer.replace((n: int) -> int { helper(n) + n })

            // Inner scope NOT active: outer.replace(5) → helper(5) + 5
            // = real helper(5) + 5 = 50 + 5 = 55
            let r1 = m_outer.scope<int, never>(() -> int throws never {
                call_outer(5)
            })

            // Inner scope IS active: outer.replace(5) → helper(5) + 5
            // = mocked helper(5) + 5 = 999 + 5 = 1004
            let r2 = m_outer.scope<int, never>(() -> int throws never {
                m_helper.scope<int, never>(() -> int throws never {
                    call_outer(5)
                })
            })

            r1 * 10000 + r2 * 10 + m_outer.call_count + m_helper.call_count * 100
        }
    "#
    );

    // r1 = 55, r2 = 1004
    // m_outer.call_count = 2 (one per scope)
    // m_helper.call_count = 1 (only fired in r2's scope)
    // result = 55 * 10_000 + 1004 * 10 + 2 + 100 = 550_000 + 10_040 + 2 + 100 = 560_142
    assert_eq!(output.result, Ok(BexExternalValue::Int(560_142)));
}

/// `Mock.new` rejects non-callable targets with a `baml.errors.InvalidArgument`.
/// Stand-in for the generic-bound constraint we'd have if BAML supported
/// `class Mock<T> where T: callable`.
#[tokio::test]
async fn mock_new_rejects_non_callable_target() {
    let output = baml_test!(
        r#"
        function main() -> string {
            let r = {
                let _ = baml.mock.Mock.new(42)
                "constructed"
            } catch (e) {
                baml.errors.InvalidArgument => "rejected",
            }
            r
        }
    "#
    );

    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("rejected".to_string()))
    );
}

/// `baml.mock.push_override` rejects non-callable arguments directly
/// (with `baml.errors.InvalidArgument`), instead of silently no-oping.
/// The legitimate caller (`Mock.scope`) re-raises this as a `baml.sys.panic`
/// because `Mock.new`/`replace` already guarantee callability — see the
/// runtime guard in `package_baml::mock::push_override`.
#[tokio::test]
async fn mock_push_override_rejects_non_callable_target() {
    let output = baml_test!(
        r#"
        function main() -> string {
            let r = {
                let _ = baml.mock.push_override<int>(42, 99, null)
                "accepted"
            } catch (e) {
                baml.errors.InvalidArgument => "rejected",
            }
            r
        }
    "#
    );

    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("rejected".to_string()))
    );
}

#[tokio::test]
async fn mock_push_override_rejects_non_callable_object() {
    let output = baml_test!(
        r#"
        function main() -> string {
            // Arrays are objects but not callables — push_override must
            // catch this even though the `Value::Object` discriminant
            // alone matches.
            let arr = [1, 2, 3]
            let r = {
                let _ = baml.mock.push_override<int[]>(arr, arr, null)
                "accepted"
            } catch (e) {
                baml.errors.InvalidArgument => "rejected",
            }
            r
        }
    "#
    );

    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("rejected".to_string()))
    );
}

/// Regression for an older `is_spy` heuristic that compared the
/// replacement's normalized inner-function pointer to the target's. Any
/// `replace(impl)` where `impl` reduces to the same inner function — a
/// re-export, a closure-over-target, a `BoundMethod` sharing the same
/// inner fn — would be silently treated as spy mode and the recursion
/// guard would be disabled. Replacement is now an explicit `Option`
/// (`null` = spy, `Some` = real replacement), so even
/// `replace(target_itself)` is treated as a real replacement: calls into
/// the target are suppressed, and a re-entry inside the replacement
/// triggers the `MockRecursion` panic.
#[tokio::test]
async fn mock_replace_with_same_callable_triggers_recursion_guard() {
    let output = baml_test!(
        r#"
        function target(n: int) -> int throws unknown {
            if (n <= 0) { 0 } else { target(n - 1) }
        }

        function main() -> string {
            let m = baml.mock.Mock.new(target)
            // Replacement is literally the target — under the old
            // `is_spy = (callable_identity(replacement) == target_fn)`
            // heuristic this would silently demote to spy mode and let
            // the natural recursion run uninterrupted. After the fix,
            // any non-null replacement engages the recursion guard.
            m.replace(target)
            let r = {
                m.scope<int, unknown>(() -> int throws unknown {
                    target(2)
                })
                "ran-without-guard"
            } catch (e) {
                baml.panics.UserPanic => "guard-fired",
            }
            r
        }
    "#
    );

    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("guard-fired".to_string())),
    );
}

/// Regression: when a replacement throws and the exception is caught
/// *inside* the active scope (i.e. upstream of the mock dispatch but
/// downstream of the user's `catch`), the recursion-guard suppression
/// flag must be cleared so subsequent intercepted calls still resolve
/// through the replacement.
///
/// Before the fix, `try_unwind_exception` popped `Frame::Native(_)`
/// frames during unwinding without invoking their continuations, so
/// `UnsuppressMockContinuation` never ran on the throw path. The
/// matching `mock_stack` entry stayed `suppressed = true` for the rest
/// of the scope; the next call to the target would walk past the
/// suppressed entry, see `saw_suppressed_match`, and throw a
/// `MockRecursion` panic instead of dispatching the replacement.
#[tokio::test]
async fn mock_replacement_throw_caught_inside_scope_clears_suppression() {
    let output = baml_test!(
        r#"
        function target(n: int) -> int throws unknown { n }

        function main() -> int {
            let m = baml.mock.Mock.new(target)
            // First intercepted call throws; the second returns 100 + n.
            m.replace((n: int) -> int throws unknown {
                if (n == 1) {
                    throw "first"
                } else {
                    100 + n
                }
            })

            let r = m.scope<int, unknown>(() -> int throws unknown {
                // First call: replacement throws "first", caught here.
                let a = { target(1) } catch_all (e) { _ => -1 }
                // Second call: must still dispatch the replacement
                // (suppression must have been cleared on unwind).
                let b = target(2)
                a * 1000 + b
            })

            // a = -1 (throw caught), b = 102 (replacement returned 100+2).
            // Result: -1 * 1000 + 102 = -898.
            r
        }
    "#
    );

    assert_eq!(output.result, Ok(BexExternalValue::Int(-898)));
}

/// Regression: when the user passes an `Object::BoundMethod` as the
/// replacement, the dispatcher must inject the bound receiver into the
/// operand stack before invoking the inner function. The visible
/// (self-less) arity is what's already on the stack at the call site;
/// the inner function's arity includes `self`, so dispatching the
/// BoundMethod without prepending the receiver mis-aligns every
/// argument (or underflows the stack outright).
///
/// Before the fix, every mock-redirect site called
/// `resolve_callable_target(replacement)` (which reports the full
/// arity for a BoundMethod) and then forwarded that count to
/// `execute_call_from_locals_offset` without inserting the receiver.
/// The result was either `InvalidArgumentCount` / stack underflow or,
/// worse, a silent off-by-one that read a stale slot as `self`.
#[tokio::test]
async fn mock_replacement_can_be_a_bound_method() {
    let output = baml_test!(
        r#"
        class C {
            value int
            function method(self, n: int) -> int { self.value * 100 + n }
        }

        function target(n: int) -> int { n }
        function call_target(n: int) -> int { target(n) }

        function main() -> int {
            let c = C { value: 7 }
            let m = baml.mock.Mock.new(target)
            // BoundMethod replacement: the dispatcher must inject `c`
            // as `self` so `method` sees (c, 5) and computes 7*100 + 5.
            m.replace(c.method)

            let r = m.scope<int, never>(() -> int throws never {
                call_target(5)
            })
            // 705 = c.method(5) with c.value = 7.
            r
        }
    "#
    );

    assert_eq!(output.result, Ok(BexExternalValue::Int(705)));
}

/// The override is popped even when the body throws.
#[tokio::test]
async fn mock_scope_pops_on_throw() {
    let output = baml_test!(
        r#"
        function original(n: int) -> int { n + 1 }
        function call_original(n: int) -> int { original(n) }

        function main() -> int {
            let m = baml.mock.Mock.new(original);
            m.replace((n: int) -> int { n + 100 });
            let _ = {
                m.scope<int, unknown>(() -> int throws unknown {
                    throw "boom"
                })
            } catch_all (e) {
                _ => 0,
            }
            call_original(7)
        }
    "#
    );

    assert_eq!(output.result, Ok(BexExternalValue::Int(8)));
}
