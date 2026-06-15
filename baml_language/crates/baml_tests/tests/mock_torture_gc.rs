//! BEP-058 function mocking — GC TORTURE pass.
//!
//! These are EXTREME / SAD-PATH cases that hammer the GC rooting and forwarding
//! of mock state. Each program forces a Gen0 collection (heavy `while`-loop
//! allocation of arrays / strings) at the most dangerous moment — mid-scope,
//! mid-replacement, mid-spawn, right before the mocked target is called — so the
//! `Object::Mock`, its `Instance`-key receiver, or its replacement closure is
//! relocated under the live scope. The dispatch that follows must still resolve
//! correctly (no use-after-move, no wrong-value, no crash).
//!
//! Predictions are traced from the BEP-058 semantics and the implemented GC
//! contract: `collect_roots`/`forward_roots` root + re-key the `mock_table`
//! (mock ptrs and `FunctionKey::Instance` receivers) and `mock_suppress`; the
//! GC's own `Object::Mock` tracer keeps the replacement closure alive and
//! forwards the baked-in receiver in lockstep; spawns seed the child table from
//! a clone of the parent's (shared heap objects). If any of that breaks, these
//! tests surface a corrupted result, a crash, or (for the recursion-guard cases)
//! a hang.

use baml_tests::baml_test;
use bex_engine::BexExternalValue;

/// Heavy alloc INSIDE the scope body, BEFORE the mocked call: the Mock and its
/// replacement closure are relocated mid-scope, then the mocked target is hit.
/// The forwarded replacement must still produce 7.
#[tokio::test]
async fn torture_gc_01_alloc_in_scope_before_call() {
    let output = baml_test!(
        r#"
        function target() -> int { 1 }

        function main() -> int {
            let m = baml.mock.new(target);
            m.replace(() -> int { 7 });
            let r = 0;
            baml.mock.scope(m, () -> void {
                let i = 0;
                while (i < 4000) {
                    let arr = [i, i * 2, i * 3];
                    i = i + 1;
                }
                r = target();          // mock + replacement moved, must still -> 7
            });
            r
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(7)));
}

/// Heavy alloc INSIDE the replacement body BEFORE it returns: the Mock object is
/// live in the table while its own replacement frame allocates and triggers GC.
/// The replacement must complete and the value survive.
#[tokio::test]
async fn torture_gc_02_alloc_inside_replacement_body() {
    let output = baml_test!(
        r#"
        function target() -> int { 1 }

        function main() -> int {
            let m = baml.mock.new(target);
            m.replace(() -> int {
                let acc = 0;
                let i = 0;
                while (i < 4000) {
                    let arr = [i, i + 1, i + 2];
                    acc = acc + 1;
                    i = i + 1;
                }
                acc                    // 4000, computed across GC cycles
            });
            let r = 0;
            baml.mock.scope(m, () -> void {
                r = target();
            });
            r
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(4000)));
}

/// Instance mock whose receiver is allocated, then surrounded by heavy alloc,
/// then called. The `FunctionKey::Instance(receiver, _)` and the receiver baked
/// into the Mock's own `function_key` must both be forwarded in lockstep, or the
/// dispatch keys on a stale pointer and misses (real -> 6 instead of -1).
#[tokio::test]
async fn torture_gc_03_instance_receiver_moved_under_scope() {
    let output = baml_test!(
        r#"
        class Counter {
          count int
          function bump(self) -> int { self.count + 1 }
        }

        function main() -> int {
            let a = Counter { count: 5 };
            let m = baml.mock.new(a.bump);     // Instance key on `a`
            m.replace(() -> int { -1 });
            let r = 0;
            baml.mock.scope(m, () -> void {
                let i = 0;
                while (i < 4000) {
                    let arr = [i, i, i];       // move `a` + the Mock mid-scope
                    i = i + 1;
                }
                r = a.bump();                  // must still hit the instance mock -> -1
            });
            r
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(-1)));
}

/// A mock reachable ONLY via the table: the local binding is rebound to an int,
/// so the table is the sole root. Heavy alloc must keep the Mock alive (rooted
/// by `collect_roots` from `mock_table`) — if it weren't rooted, the call inside
/// the scope reads a collected/moved Mock.
#[tokio::test]
async fn torture_gc_04_mock_rooted_only_via_table() {
    let output = baml_test!(
        r#"
        function target() -> int { 1 }

        function main() -> int {
            let m = baml.mock.new(target);
            m.replace(() -> int { 42 });
            let r = 0;
            baml.mock.scope(m, () -> void {
                m = 0;                         // local no longer points at the Mock
                let i = 0;
                while (i < 4000) {
                    let arr = [i, i * 2, i * 3];
                    i = i + 1;
                }
                r = target();                  // Mock alive only via the table -> 42
            });
            r
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(42)));
}

/// Deeply nested mock stack (four levels on one key) with heavy alloc at the
/// BOTTOM, then the innermost call. Every stack entry is rooted; the per-key
/// stack must survive a move and still resolve innermost-wins.
#[tokio::test]
async fn torture_gc_05_deep_stack_alloc_at_bottom() {
    let output = baml_test!(
        r#"
        function greeting() -> string { "real" }

        function main() -> string {
            let a = baml.mock.new(greeting); a.replace(() -> string { "a" });
            let b = baml.mock.new(greeting); b.replace(() -> string { "b" });
            let c = baml.mock.new(greeting); c.replace(() -> string { "c" });
            let d = baml.mock.new(greeting); d.replace(() -> string { "d" });
            let r = "";
            baml.mock.scope(a, () -> void {
                baml.mock.scope(b, () -> void {
                    baml.mock.scope(c, () -> void {
                        baml.mock.scope(d, () -> void {
                            let i = 0;
                            while (i < 4000) {
                                let arr = [i, i, i];   // GC under 4 stacked mocks
                                i = i + 1;
                            }
                            r = greeting();            // innermost d -> "d"
                        });
                    });
                });
            });
            r
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::String("d".into())));
}

/// Array-form scope `scope([a,b,c], ...)` with heavy alloc inside the body. All
/// three mocks were activated in one `__enter`; the table holds three live
/// pointers that must all be forwarded. Innermost (c) is a spy delegating down
/// to b's replacement.
#[tokio::test]
async fn torture_gc_06_array_scope_alloc_then_delegate() {
    let output = baml_test!(
        r#"
        function greeting() -> string { "real" }

        function main() -> int {
            let a = baml.mock.new(greeting); a.replace(() -> string { "A" });
            let b = baml.mock.new(greeting); b.replace(() -> string { "B[" + greeting() + "]" });
            let c = baml.mock.new(greeting);   // pure spy, innermost
            let r = "";
            baml.mock.scope([a, b, c], () -> void {
                let i = 0;
                while (i < 4000) {
                    let arr = [i, i, i];
                    i = i + 1;
                }
                r = greeting();                // c (spy) -> b -> "B[A]" ; len 4
            });
            r.length() * 100 + c.call_count * 10 + b.call_count
            // "B[A]"(4)*100 + c=2*10 + b=1 = 421
            // c is a transparent spy: it re-counts on b's re-entrant greeting()
            // — only the running replacement is suppressed, not spies above it
            // (see torture_precedence_conflict_08/12, torture_recursion_07).
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(421)));
}

/// A replacement that is a LARGE capturing closure (closes over a big array),
/// GC pressure, then called. The closure (and its captured upvalue) is reachable
/// only through the Mock's `replacement` slot — the GC's `Object::Mock` tracer
/// must keep it alive and forward it.
#[tokio::test]
async fn torture_gc_07_large_capturing_replacement_closure() {
    let output = baml_test!(
        r#"
        function target() -> int { 0 }

        function main() -> int {
            let captured = [11, 22, 33, 44, 55];
            let m = baml.mock.new(target);
            // Closure captures `captured`; reachable only via the Mock slot.
            m.replace(() -> int { captured[0] + captured[4] });   // 11 + 55 = 66
            let r = 0;
            baml.mock.scope(m, () -> void {
                let i = 0;
                while (i < 4000) {
                    let arr = [i, i, i];
                    i = i + 1;
                }
                r = target();              // captured array survived -> 66
            });
            r
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(66)));
}

/// Heavy alloc INSIDE a spawn body BEFORE calling the mocked target. The child
/// VM's seeded `mock_table` must root the Mock across the child's own GC, and
/// the shared heap Mock object stays valid.
#[tokio::test]
async fn torture_gc_08_alloc_in_spawn_before_mocked_call() {
    let output = baml_test!(
        r#"
        function target() -> int { 1 }

        function main() -> int {
            let m = baml.mock.new(target);
            m.replace(() -> int { 99 });
            let r = 0;
            baml.mock.scope(m, () -> void {
                let f = spawn {
                    let i = 0;
                    while (i < 4000) {
                        let arr = [i, i, i];   // GC inside the child VM
                        i = i + 1;
                    }
                    target()                   // child's table still has m -> 99
                };
                r = await f;
            });
            r * 10 + m.call_count              // 99*10 + 1 = 991
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(991)));
}

/// Detached spawn: instance mock, receiver moved by alloc in BOTH parent (before
/// spawn) and child (before call). The receiver pointer must round-trip the
/// parent's table re-key, the spawn snapshot clone, and the child's own GC.
#[tokio::test]
async fn torture_gc_09_detached_spawn_instance_receiver_moved_twice() {
    let output = baml_test!(
        r#"
        class Counter {
          count int
          function bump(self) -> int { self.count + 1 }
        }

        function main() -> int {
            let a = Counter { count: 100 };
            let m = baml.mock.new(a.bump);
            m.replace(() -> int { -7 });
            let held = spawn { 0 };
            baml.mock.scope(m, () -> void {
                let i = 0;
                while (i < 3000) {
                    let arr = [i, i, i];        // move `a` in the parent
                    i = i + 1;
                }
                held = spawn {
                    let j = 0;
                    while (j < 3000) {
                        let arr2 = [j, j, j];    // move `a` again in the child
                        j = j + 1;
                    }
                    a.bump()                     // snapshot+forwarded receiver -> -7
                };
            });
            await held
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(-7)));
}

/// Recursion guard UNDER GC pressure: the replacement calls the target by name
/// AND allocates heavily before doing so. The by-name re-entry must step one
/// down to the real function (suppress entry survives), not loop. If the
/// suppress-pointer forwarding is wrong, this could recurse forever.
#[tokio::test]
async fn torture_gc_10_recursion_guard_with_alloc() {
    let output = baml_test!(
        r#"
        function f(x: int) -> int { x }

        function main() -> int {
            let m = baml.mock.new(f);
            m.replace((x: int) -> int {
                let i = 0;
                while (i < 4000) {
                    let arr = [i, i, i];   // GC while the suppress entry is live
                    i = i + 1;
                }
                f(x) + 1                    // by-name re-entry -> real f(x) -> x+1
            });
            let r = 0;
            baml.mock.scope(m, () -> void {
                r = f(5);                   // 5 + 1 = 6
            });
            r
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(6)));
}

/// Long-string GC pressure (not arrays): build a long string by repeated concat
/// inside the scope, forcing many string allocations and a move of the Mock,
/// then call the mocked string-returning function.
#[tokio::test]
async fn torture_gc_11_long_string_pressure_then_call() {
    let output = baml_test!(
        r#"
        function greeting() -> string { "real" }

        function main() -> int {
            let m = baml.mock.new(greeting);
            m.replace(() -> string { "mocked" });
            let r = "";
            baml.mock.scope(m, () -> void {
                let s = "";
                let i = 0;
                while (i < 3000) {
                    s = s + "x";           // grows a long string, heavy alloc
                    i = i + 1;
                }
                r = greeting();            // Mock moved -> "mocked" (len 6)
            });
            r.length()
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(6)));
}

/// Mock activated, scope EXITED (table empty), then heavy alloc, then the SAME
/// mock re-entered and called. The Mock object must survive the dormant period
/// (rooted by the live local) and re-activate cleanly with a fresh count.
#[tokio::test]
async fn torture_gc_12_reuse_mock_after_exit_with_alloc() {
    let output = baml_test!(
        r#"
        function target() -> int { 1 }

        function main() -> int {
            let m = baml.mock.new(target);
            m.replace(() -> int { 3 });
            baml.mock.scope(m, () -> void {
                let _ = target();          // count -> 1
            });
            // Scope exited; table empty. Heavy alloc with the Mock dormant.
            let i = 0;
            while (i < 4000) {
                let arr = [i, i, i];
                i = i + 1;
            }
            let r = 0;
            baml.mock.scope(m, () -> void {
                r = target();              // re-entered -> 3, count reset -> 1
            });
            r * 10 + m.call_count          // 3*10 + 1 = 31
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(31)));
}

/// Instance mock with a SPY (no replacement) over heavy alloc: the receiver is
/// moved, then the spy must still match (count bumped) and delegate to the real
/// method, which reads the moved instance's field correctly.
#[tokio::test]
async fn torture_gc_13_instance_spy_receiver_moved() {
    let output = baml_test!(
        r#"
        class Counter {
          count int
          function bump(self) -> int { self.count + 1 }
        }

        function main() -> int {
            let a = Counter { count: 41 };
            let m = baml.mock.new(a.bump);   // pure spy, instance key
            let r = 0;
            baml.mock.scope(m, () -> void {
                let i = 0;
                while (i < 4000) {
                    let arr = [i, i, i];     // move `a` and the Mock
                    i = i + 1;
                }
                r = a.bump();                // spy delegates to real -> 42
            });
            r * 10 + m.call_count            // 42*10 + 1 = 421
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(421)));
}

/// Nested scopes of the SAME mock with heavy alloc between activations. The mock
/// appears twice in its per-key stack; after a move the `seen` dedup and the
/// shared call_count must remain consistent (no double-count, count survives).
#[tokio::test]
async fn torture_gc_14_same_mock_nested_with_alloc_between() {
    let output = baml_test!(
        r#"
        function target() -> int { 0 }

        function main() -> int {
            let m = baml.mock.new(target);
            m.replace(() -> int { 0 });
            baml.mock.scope(m, () -> void {
                let _ = target();                 // count -> 1
                let i = 0;
                while (i < 3000) {
                    let arr = [i, i, i];          // GC with m twice in the stack
                    i = i + 1;
                }
                baml.mock.scope(m, () -> void {   // same mock nested
                    let _ = target();             // count -> 2 (single observe)
                });
                let _ = target();                 // count -> 3
            });
            m.call_count
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(3)));
}

/// Many concurrent spawns each allocating heavily before the mocked call. Every
/// child seeds and roots its own copy of the table; the shared atomic count must
/// aggregate all spawned calls despite per-child GC churn.
#[tokio::test]
async fn torture_gc_15_concurrent_spawns_each_alloc_then_call() {
    let output = baml_test!(
        r#"
        function target() -> int { 1 }

        function main() -> int {
            let m = baml.mock.new(target);
            m.replace(() -> int { 5 });
            let total = 0;
            baml.mock.scope(m, () -> void {
                let f1 = spawn {
                    let i = 0;
                    while (i < 2500) { let a = [i, i, i]; i = i + 1; }
                    target()
                };
                let f2 = spawn {
                    let i = 0;
                    while (i < 2500) { let a = [i, i, i]; i = i + 1; }
                    target()
                };
                let f3 = spawn {
                    let i = 0;
                    while (i < 2500) { let a = [i, i, i]; i = i + 1; }
                    target()
                };
                total = (await f1) + (await f2) + (await f3);   // 5+5+5 = 15
            });
            total + m.call_count                                 // 15 + 3 = 18
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(18)));
}

/// Replacement opens an INNER scope, and heavy alloc happens inside the inner
/// scope body before the inner mocked call. Two distinct Mocks (outer + inner)
/// must both survive a move while both are active on the same key.
#[tokio::test]
async fn torture_gc_16_replacement_opens_inner_scope_with_alloc() {
    let output = baml_test!(
        r#"
        function greeting() -> string { "real" }

        function main() -> string {
            let inner = baml.mock.new(greeting);
            inner.replace(() -> string { "I" });
            let outer = baml.mock.new(greeting);
            outer.replace(() -> string {
                let captured = "";
                baml.mock.scope(inner, () -> void {
                    let i = 0;
                    while (i < 3000) {
                        let arr = [i, i, i];   // GC with outer+inner both active
                        i = i + 1;
                    }
                    captured = greeting();      // inner -> "I"
                });
                "O[" + captured + "]"
            });
            let r = "";
            baml.mock.scope(outer, () -> void {
                r = greeting();                 // "O[I]"
            });
            r
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::String("O[I]".into())));
}

/// Replacement swapped MID-SCOPE after heavy alloc, then called. The first
/// replacement closure becomes garbage; the new one (allocated post-GC) must be
/// stored via the write barrier and dispatched. Stresses `set_replacement` +
/// barrier under a moved Mock.
#[tokio::test]
async fn torture_gc_17_replace_swapped_midscope_after_alloc() {
    let output = baml_test!(
        r#"
        function target() -> int { 1 }

        function main() -> int {
            let m = baml.mock.new(target);
            m.replace(() -> int { 10 });
            let r1 = 0;
            let r2 = 0;
            baml.mock.scope(m, () -> void {
                r1 = target();                 // first replacement -> 10
                let i = 0;
                while (i < 4000) {
                    let arr = [i, i, i];       // GC; Mock + new closure move
                    i = i + 1;
                }
                m.replace(() -> int { 20 });   // swap mid-scope (post-GC alloc)
                r2 = target();                 // new replacement -> 20
            });
            r1 * 100 + r2                       // 10*100 + 20 = 1020
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(1020)));
}

/// Spy stacked over a replacement, both moved by alloc, then a recursion-guarded
/// delegation chain runs. The spy must stay transparent and the suppress stack
/// must step correctly down to the real function after the move (else wrong
/// value or unbounded recursion).
#[tokio::test]
async fn torture_gc_18_spy_over_delegating_replacement_with_alloc() {
    let output = baml_test!(
        r#"
        function greeting() -> string { "hello" }

        function main() -> int {
            let a = baml.mock.new(greeting);
            a.replace(() -> string { "A[" + greeting() + "]" });   // delegates down
            let s = baml.mock.new(greeting);   // pure spy on top
            let r = "";
            baml.mock.scope(a, () -> void {
                baml.mock.scope(s, () -> void {
                    let i = 0;
                    while (i < 4000) {
                        let arr = [i, i, i];   // GC under spy+replacement+suppress
                        i = i + 1;
                    }
                    r = greeting();            // spy -> a -> "A[hello]" (len 8)
                });
            });
            r.length() * 100 + s.call_count * 10 + a.call_count
            // "A[hello]"(8)*100 + s=2*10 + a=1 = 821
            // s is a transparent spy: it re-counts on a's re-entrant greeting()
            // — only the running replacement is suppressed, not spies above it
            // (see torture_precedence_conflict_08/12, torture_recursion_07).
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(821)));
}
