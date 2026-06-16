//! BEP-058 function mocking — TORTURE pass: unwind / finally-pop.
//!
//! These are EXTREME / SAD-PATH cases that stress `baml.mock.scope`'s unwind
//! contract: "A scope ends on unwind, so a throw in the body or a replacement
//! deactivates the mock as the scope exits (finally-style). A faulting mocked
//! call still counts toward `call_count`." (BEP-058 §Errors.)
//!
//! `scope` is implemented in BAML as:
//!   __enter(mocks); body() catch (e) { _ => { __exit(mocks); throw e } }; __exit(mocks)
//! so every unwind path must route through the catch's `__exit` (which pops ALL
//! array elements in reverse) before re-raising. The torture targets: array
//! scopes that throw (every element must deactivate), throws unwinding 3+ nested
//! scopes at once, throws inside spawn bodies under active mocks, throws from a
//! replacement that itself opened an inner scope, catch-rethrow chains across
//! scope boundaries, call_count after a fault, re-entry after a throw (count
//! reset), and a replacement that throws on the 2nd call but not the 1st.

use baml_tests::baml_test;
use bex_engine::BexExternalValue;

/// Throw inside an ARRAY scope body: BOTH array elements must deactivate, so a
/// post-catch call hits the real fn (not either replacement).
#[tokio::test]
async fn torture_unwind_01_array_scope_throws_all_elements_pop() {
    let output = baml_test!(
        r#"
        function target() -> int throws string { 1 }

        function main() -> int {
            let a = baml.mock.new(target);
            a.replace(() -> int throws string { 50 });
            let b = baml.mock.new(target);
            b.replace(() -> int throws string { 70 });
            let caught = 0;
            baml.mock.scope([a, b], () -> void {
                throw "boom";          // unwinds before any call
            }) catch (e) {
                _ => { caught = 1 }
            };
            // Both a and b must have been popped by the catch's __exit.
            let after = target();      // real -> 1, not 70 (b) or 50 (a)
            caught * 1000 + after      // 1000 + 1 = 1001
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(1001)));
}

/// Throw AFTER a call inside an array scope: innermost (b) replacement claimed
/// the call (count=1 on b, 0 on a since b short-circuits), then unwind pops both.
#[tokio::test]
async fn torture_unwind_02_array_scope_call_then_throw_counts() {
    let output = baml_test!(
        r#"
        function target() -> int throws string { 1 }

        function main() -> int {
            let a = baml.mock.new(target);
            a.replace(() -> int throws string { 50 });
            let b = baml.mock.new(target);
            b.replace(() -> int throws string { 70 });
            let caught = 0;
            baml.mock.scope([a, b], () -> void {
                let _ = target();      // b (innermost) claims -> count b=1, a=0
                throw "boom";
            }) catch (e) {
                _ => { caught = 1 }
            };
            let after = target();      // both popped -> real 1
            // a=0, b=1, after=1, caught=1
            caught * 10000 + a.call_count * 1000 + b.call_count * 100 + after
            // 10000 + 0 + 100 + 1 = 10101
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(10101)));
}

/// A throw that unwinds 3 nested scopes at once, caught at the OUTERMOST scope's
/// catch. The throw originates in the innermost body; it must pop all three
/// (c, b, a) as it propagates out, so a post-catch call is real.
#[tokio::test]
async fn torture_unwind_03_three_nested_scopes_unwind_at_once() {
    let output = baml_test!(
        r#"
        function greeting() -> string throws string { "real" }

        function main() -> string {
            let a = baml.mock.new(greeting); a.replace(() -> string throws string { "a" });
            let b = baml.mock.new(greeting); b.replace(() -> string throws string { "b" });
            let c = baml.mock.new(greeting); c.replace(() -> string throws string { "c" });
            let caught = "";
            baml.mock.scope(a, () -> void {
                baml.mock.scope(b, () -> void {
                    baml.mock.scope(c, () -> void {
                        throw "deep";   // unwinds c, then b, then a
                    });
                });
            }) catch (e) {
                _ => { caught = "X" }
            };
            // All three must be popped; the next call is the real fn.
            caught + greeting()         // "X" + "real" = "Xreal"
        }
        "#
    );
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("Xreal".into()))
    );
}

/// The innermost of three nested scopes catches its OWN throw and swallows it.
/// Only c pops (its scope completed normally after the catch); a and b remain
/// active for a later call inside them. Verifies the unwind is bounded to the
/// scope whose catch handles it, not over-popping outer scopes.
#[tokio::test]
async fn torture_unwind_04_inner_catch_only_pops_inner() {
    let output = baml_test!(
        r#"
        function greeting() -> string throws string { "real" }

        function main() -> string {
            let a = baml.mock.new(greeting); a.replace(() -> string throws string { "a" });
            let b = baml.mock.new(greeting); b.replace(() -> string throws string { "b" });
            let c = baml.mock.new(greeting); c.replace(() -> string throws string { "c" });
            let log = "";
            baml.mock.scope(a, () -> void {
                baml.mock.scope(b, () -> void {
                    baml.mock.scope(c, () -> void {
                        throw "x";
                    }) catch (e) {
                        _ => { log = log + "C" }    // c's throw caught here; c pops
                    };
                    log = log + greeting();          // still inside b -> "b"
                });
                log = log + greeting();              // still inside a -> "a"
            });
            log = log + greeting();                  // all popped -> "real"
            log                                      // "Cbareal"
        }
        "#
    );
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("Cbareal".into()))
    );
}

/// A faulting mocked call still increments call_count (BEP §Errors), and after
/// the throw is caught the count is observable. Replacement throws immediately.
#[tokio::test]
async fn torture_unwind_05_faulting_call_still_counts() {
    let output = baml_test!(
        r#"
        function target() -> int throws string { 1 }

        function main() -> int {
            let m = baml.mock.new(target);
            m.replace(() -> int throws string { throw "boom" });
            let caught = 0;
            baml.mock.scope(m, () -> void {
                let _ = target();      // count incremented, then throws
            }) catch (e) {
                _ => { caught = 1 }
            };
            // call_count must be 1 even though the replacement threw.
            caught * 100 + m.call_count   // 100 + 1 = 101
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(101)));
}

/// Throw, catch, then RE-ENTER the same mock fresh: call_count resets to 0 on
/// the new scope entry (it is not still 1 from the faulting first scope).
#[tokio::test]
async fn torture_unwind_06_reenter_after_throw_resets_count() {
    let output = baml_test!(
        r#"
        function target() -> int throws string { 1 }

        function main() -> int {
            let m = baml.mock.new(target);
            m.replace(() -> int throws string { throw "boom" });
            // First scope: a faulting call bumps count to 1, then throws.
            baml.mock.scope(m, () -> void {
                let _ = target();
            }) catch (e) {
                _ => { }
            };
            let after_first = m.call_count;   // 1 (faulting call counted)
            // Re-enter fresh: count must reset to 0; one good replacement run.
            m.replace(() -> int throws string { 9 });
            let r = 0;
            baml.mock.scope(m, () -> void {
                r = target();                 // 9, count -> 1
            });
            // after_first=1, fresh count=1, r=9
            after_first * 100 + m.call_count * 10 + r   // 100 + 10 + 9 = 119
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(119)));
}

/// A replacement that throws on the 2nd call but not the 1st (it closes over a
/// counter). First call returns normally; second throws and unwinds the scope.
/// call_count must be 2 (both faulting and non-faulting count).
#[tokio::test]
async fn torture_unwind_07_replacement_throws_on_second_call() {
    let output = baml_test!(
        r#"
        function target() -> int throws string { 1 }

        function main() -> int {
            let calls = 0;
            let m = baml.mock.new(target);
            m.replace(() -> int throws string {
                calls = calls + 1;
                if calls > 1 { throw "second" } else { 42 }
            });
            let first = 0;
            let caught = 0;
            baml.mock.scope(m, () -> void {
                first = target();      // 1st -> 42
                let _ = target();      // 2nd -> throws
            }) catch (e) {
                _ => { caught = 1 }
            };
            // first=42, count=2 (both ran the replacement body), caught=1
            caught * 10000 + first * 100 + m.call_count   // 10000 + 4200 + 2 = 14202
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(14202)));
}

/// Throw from inside a SPAWN body while a mock is active: awaiting the faulted
/// future surfaces the error, but the mocked call inside the spawn still bumped
/// the shared call_count before throwing.
#[tokio::test]
async fn torture_unwind_08_throw_in_spawn_body_under_mock() {
    let output = baml_test!(
        r#"
        function target() -> int throws string { 1 }

        function main() -> int {
            let m = baml.mock.new(target);
            m.replace(() -> int throws string {
                let _ = target();      // re-entrant: suppressed -> real 1, counts? no (suppressed mock skipped)
                throw "spawn boom"
            });
            let caught = 0;
            let cnt = 0;
            baml.mock.scope(m, () -> void {
                let f = spawn { target() };   // mocked in spawn; replacement throws
                (await f) catch (e) {
                    _ => { caught = 1; 0 }
                };
                cnt = m.call_count;           // the faulting spawned call counted
            });
            caught * 10 + cnt                 // 10 + 1 = 11
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(11)));
}

/// Throw from inside a spawn body that the parent never catches: the unhandled
/// spawn error propagates and the whole program ends in Err.
#[tokio::test]
async fn torture_unwind_09_uncaught_spawn_throw_under_mock_errors() {
    let output = baml_test!(
        r#"
        function target() -> int throws string { 1 }

        function main() -> int throws string {
            let m = baml.mock.new(target);
            m.replace(() -> int throws string { throw "boom" });
            let r = 0;
            baml.mock.scope(m, () -> void {
                let f = spawn { target() };   // replacement throws inside the spawn
                r = await f;                  // propagates the throw, uncaught
            });
            r
        }
        "#
    );
    assert!(output.result.is_err());
}

/// A replacement that OPENED an inner scope then throws: both the inner scope
/// (opened by the replacement) and the outer scope must pop on unwind. After the
/// caught throw, the real fn runs (neither mock active).
#[tokio::test]
async fn torture_unwind_10_replacement_opens_inner_then_throws() {
    let output = baml_test!(
        r#"
        function greeting() -> string throws string { "real" }

        function main() -> string {
            let inner = baml.mock.new(greeting);
            inner.replace(() -> string throws string { "I" });
            let outer = baml.mock.new(greeting);
            outer.replace(() -> string throws string {
                // The replacement opens its OWN inner scope, then throws inside it.
                baml.mock.scope(inner, () -> void {
                    throw "inner boom";        // unwinds inner scope first
                });
                "unreachable"
            });
            let caught = "";
            baml.mock.scope(outer, () -> void {
                let _ = greeting();            // runs outer's replacement -> throws
            }) catch (e) {
                _ => { caught = "C" }
            };
            // Both inner (popped by its scope's catch->__exit) and outer popped.
            caught + greeting()                // "C" + "real" = "Creal"
        }
        "#
    );
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("Creal".into()))
    );
}

/// Catch-rethrow chain across scope boundaries: an inner scope catches and
/// RE-THROWS a different error, which then unwinds the outer scope. Both scopes
/// must deactivate; the final post-catch call is real.
#[tokio::test]
async fn torture_unwind_11_catch_rethrow_across_scopes() {
    let output = baml_test!(
        r#"
        function greeting() -> string throws string { "real" }

        function main() -> string {
            let a = baml.mock.new(greeting); a.replace(() -> string throws string { "a" });
            let b = baml.mock.new(greeting); b.replace(() -> string throws string { "b" });
            let outer_caught = "";
            baml.mock.scope(a, () -> void {
                baml.mock.scope(b, () -> void {
                    throw "first";
                }) catch (e) {
                    _ => { throw "second" }   // re-throw a NEW error; unwinds scope a
                };
            }) catch (e) {
                _ => { outer_caught = "O" }
            };
            // b popped by its catch, a popped by its catch. Real fn now.
            outer_caught + greeting()          // "O" + "real" = "Oreal"
        }
        "#
    );
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("Oreal".into()))
    );
}

/// Deep array (4 elements) scope that throws after exercising the innermost:
/// every one of the 4 must pop on unwind. Verified by a later real call plus
/// each element's call_count (only the innermost, d, claimed the single call).
#[tokio::test]
async fn torture_unwind_12_four_element_array_throws_all_pop() {
    let output = baml_test!(
        r#"
        function greeting() -> string throws string { "real" }

        function main() -> int {
            let a = baml.mock.new(greeting); a.replace(() -> string throws string { "a" });
            let b = baml.mock.new(greeting); b.replace(() -> string throws string { "b" });
            let c = baml.mock.new(greeting); c.replace(() -> string throws string { "c" });
            let d = baml.mock.new(greeting); d.replace(() -> string throws string { "d" });
            let claimed = "";
            let caught = 0;
            baml.mock.scope([a, b, c, d], () -> void {
                claimed = greeting();   // d innermost claims -> "d", count d=1 others 0
                throw "boom";
            }) catch (e) {
                _ => { caught = 1 }
            };
            let after = greeting();     // all 4 popped -> "real" (len 4)
            // counts: a=0 b=0 c=0 d=1; claimed len=1; after len=4; caught=1
            caught * 100000
              + after.length() * 10000
              + claimed.length() * 1000
              + a.call_count * 100 + b.call_count * 10 + c.call_count
              + d.call_count
            // 100000 + 40000 + 1000 + 0 + 1 = 141001
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(141001)));
}

/// Recursion-guard under unwind: a replacement calls the target BY NAME (steps
/// one down to the real fn, which throws), and the replacement does NOT catch it.
/// The throw unwinds out of the replacement, out of the scope (popping the mock),
/// and is caught in main. Must TERMINATE (suppress steps one down), not hang.
#[tokio::test]
async fn torture_unwind_13_replacement_calls_throwing_original_unwinds() {
    let output = baml_test!(
        r#"
        function target() -> int throws string { throw "real boom" }

        function main() -> int {
            let m = baml.mock.new(target);
            m.replace(() -> int throws string {
                target()       // steps down to real (suppressed) -> throws; unguarded
            });
            let caught = 0;
            baml.mock.scope(m, () -> void {
                let _ = target();   // replacement runs, calls real, real throws
            }) catch (e) {
                _ => { caught = 1 }
            };
            // mock popped on unwind; real call now also throws but we don't call it.
            caught * 10 + m.call_count    // 10 + 1 = 11
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(11)));
}

/// Pure SPY over a throwing replacement: the spy is transparent and delegates
/// down to the replacement, which throws. Both the spy and replacement counted
/// before the throw; unwind pops both. (Spy claims nothing, so it counts and
/// keeps delegating per the dispatch loop.)
#[tokio::test]
async fn torture_unwind_14_spy_over_throwing_replacement_unwinds() {
    let output = baml_test!(
        r#"
        function target() -> int throws string { 1 }

        function main() -> int {
            let r = baml.mock.new(target);
            r.replace(() -> int throws string { throw "boom" });   // bottom replacement
            let s = baml.mock.new(target);                          // pure spy on top
            let caught = 0;
            baml.mock.scope(r, () -> void {
                baml.mock.scope(s, () -> void {
                    let _ = target();   // spy counts + delegates -> replacement throws
                });
            }) catch (e) {
                _ => { caught = 1 }
            };
            // Both scopes popped; spy counted once, replacement counted once.
            caught * 1000 + s.call_count * 100 + r.call_count * 10 + target()
            // 1000 + 100 + 10 + 1 = 1111
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(1111)));
}

/// Unwind through a scope whose body opened ANOTHER nested scope (3 deep) and a
/// spawn at the deepest level; the spawn's replacement throws and is awaited
/// uncaught, taking down the whole program. Verifies an uncaught spawn throw
/// under stacked mocks still surfaces Err (no swallow).
#[tokio::test]
async fn torture_unwind_15_uncaught_spawn_throw_in_deep_nesting_errors() {
    let output = baml_test!(
        r#"
        function greeting() -> string throws string { "real" }

        function main() -> string throws string {
            let a = baml.mock.new(greeting); a.replace(() -> string throws string { "a" });
            let b = baml.mock.new(greeting); b.replace(() -> string throws string { "b" });
            let c = baml.mock.new(greeting);
            c.replace(() -> string throws string { throw "deep spawn boom" });
            let r = "";
            baml.mock.scope(a, () -> void {
                baml.mock.scope(b, () -> void {
                    baml.mock.scope(c, () -> void {
                        let f = spawn { greeting() };   // c innermost -> throws in spawn
                        r = await f;                    // uncaught -> propagates
                    });
                });
            });
            r
        }
        "#
    );
    assert!(output.result.is_err());
}

/// Throw originates in a mocked call deep in a CHAIN of real functions inside the
/// scope (main_body -> level1 -> level2 -> mocked target), unwinding multiple
/// real frames AND the scope. The mock must still pop (post-catch call is real)
/// and the faulting call counted.
#[tokio::test]
async fn torture_unwind_16_throw_unwinds_real_frames_and_scope() {
    let output = baml_test!(
        r#"
        function target() -> int throws string { 1 }
        function level2() -> int throws string { target() }
        function level1() -> int throws string { level2() }

        function main() -> int {
            let m = baml.mock.new(target);
            m.replace(() -> int throws string { throw "boom" });
            let caught = 0;
            baml.mock.scope(m, () -> void {
                let _ = level1();   // level1 -> level2 -> target(mocked) -> throws
            }) catch (e) {
                _ => { caught = 1 }
            };
            let after = target();   // mock popped -> real 1
            caught * 1000 + m.call_count * 10 + after   // 1000 + 10 + 1 = 1011
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(1011)));
}

/// Instance-mock array unwind: an array of an instance mock and a class mock on
/// the same method; the body throws after a call. The instance mock (more
/// specific, innermost) claims the call; both pop on unwind so a later call is
/// real. Stresses array __exit popping mixed-key entries in reverse.
#[tokio::test]
async fn torture_unwind_17_instance_and_class_array_unwind() {
    let output = baml_test!(
        r#"
        class Counter {
          count int
          function bump(self) -> int throws string { self.count + 1 }
        }

        function main() -> int {
            let a = Counter { count: 0 };
            let cls = baml.mock.new(Counter.bump);
            cls.replace((self: Counter) -> int throws string { 5 });
            let inst = baml.mock.new(a.bump);
            inst.replace(() -> int throws string { 7 });
            let claimed = 0;
            let caught = 0;
            // array: cls first, inst innermost (more specific + innermost both -> inst).
            baml.mock.scope([cls, inst], () -> void {
                claimed = a.bump();   // instance mock wins -> 7
                throw "boom";
            }) catch (e) {
                _ => { caught = 1 }
            };
            let after = a.bump();     // both popped -> real -> 1
            // claimed=7, after=1, inst count=1, cls count=0, caught=1
            caught * 100000 + claimed * 1000 + after * 100
              + inst.call_count * 10 + cls.call_count
            // 100000 + 7000 + 100 + 10 + 0 = 107110
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(107110)));
}

/// Re-entry storm: throw out of a scope, catch, re-enter the SAME mock, throw
/// again, catch, then run normally. Each entry resets call_count; the mock table
/// must be clean (no leaked stack entries) so the final real/replacement
/// behavior is correct across repeated unwinds.
#[tokio::test]
async fn torture_unwind_18_repeated_throw_reenter_no_leak() {
    let output = baml_test!(
        r#"
        function target() -> int throws string { 1 }

        function main() -> int {
            let m = baml.mock.new(target);
            m.replace(() -> int throws string { throw "boom" });
            // Round 1: faulting call, count -> 1, then throw out.
            baml.mock.scope(m, () -> void {
                let _ = target();
            }) catch (e) { _ => { } };
            let c1 = m.call_count;     // 1
            // Round 2: faulting call again; fresh entry resets count to 1.
            baml.mock.scope(m, () -> void {
                let _ = target();
            }) catch (e) { _ => { } };
            let c2 = m.call_count;     // 1 (reset, not 2)
            // Round 3: now no throw, two good calls -> count 2.
            m.replace(() -> int throws string { 0 });
            baml.mock.scope(m, () -> void {
                let _ = target();
                let _ = target();
            });
            let c3 = m.call_count;     // 2
            // After all scopes popped, the mock must be fully inactive: real fn.
            let after = target();      // real -> 1
            c1 * 1000 + c2 * 100 + c3 * 10 + after   // 1000 + 100 + 20 + 1 = 1121
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(1121)));
}
