//! BEP-058: function mocking — recursion-guard & suppression-depth TORTURE.
//!
//! These push the recursion guard (`mock_suppress`: Vec of (HeapPtr, frame_depth),
//! pruned by `depth > stored`, pushed on Redirect) and the by-name re-entry rule
//! ("calling the target by name re-enters the head and is stopped by the guard;
//! super steps down instead") to the breaking point. Deep self-recursion, mutual
//! recursion, 3-way cycles, inner-scope re-binding mid-recursion, deeper re-mocks,
//! spies wedged into recursive chains, and recursion across try/catch + spawn.
//!
//! Each result is predicted by tracing the BEP semantics (the spec, not the impl):
//! by-name re-entry steps EXACTLY one layer down to the real fn at the bottom, the
//! guard prunes only when its frame returns, and no chain may infinite-loop.

use baml_tests::baml_test;
use bex_engine::BexExternalValue;

/// Deep self-recursion: a replacement that re-invokes the target by name should
/// step ONE layer down to the real fn (not re-enter itself), so a 40-deep
/// "recursive" replacement collapses to a single real call + 40 increments? No:
/// the guard fires on the FIRST by-name re-entry, so the replacement runs once,
/// its single inner `f(n)` reaches the real fn, and there is no further descent.
/// Real f(n) = n. Replacement returns f(7)+1 = 8. Predict 8, NOT a 40-level pile.
#[tokio::test]
async fn torture_recursion_01_self_recursion_guard_collapses_to_one_real() {
    let output = baml_test!(
        r#"
        function f(x: int) -> int { x }

        function main() -> int {
            let m = baml.mock.new(f);
            m.replace((x: int) -> int { f(x) + 1 });
            let r = 0;
            baml.mock.scope(m, () -> void {
                r = f(7);            // replacement: f(7) re-enters head -> guard -> real 7; +1 = 8
            });
            r
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(8)));
}

/// Recursive replacement that genuinely loops on a DIFFERENT counter the real fn
/// reads, to prove the guard does not "use up" after one call. The replacement
/// calls the real f exactly once per invocation; but it is itself only invoked
/// once (the outer call). A real, decreasing recursion lives entirely in the real
/// fn. real f(n) = sum 0..n. f.replace adds 1000 to whatever the real chain
/// returns. Outer f(5): replacement -> 1000 + real_f(5). real_f(5)=5+4+3+2+1+0=15.
/// -> 1015. The recursion inside real_f never re-triggers the replacement because
/// the by-name calls happen inside the REAL fn's frames (target not re-entered as
/// head; guard still suppresses since the same mock is on the stack the whole
/// dynamic extent).
#[tokio::test]
async fn torture_recursion_02_replacement_wraps_real_recursive_fn() {
    let output = baml_test!(
        r#"
        function sum_to(n: int) -> int {
            if n <= 0 { 0 } else { n + sum_to(n - 1) }
        }

        function main() -> int {
            let m = baml.mock.new(sum_to);
            m.replace((n: int) -> int { 1000 + sum_to(n) });
            let r = 0;
            baml.mock.scope(m, () -> void {
                r = sum_to(5);   // 1000 + (real recursion all suppressed) = 1000 + 15
            });
            r
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(1015)));
}

/// 2-way mutual recursion, BOTH mocked: f.replace calls g, g.replace calls f.
/// f mocked -> g mocked -> f's mock is suppressed (deeper frame) -> real f.
/// scope([f_mock, g_mock]); call f():
///   f_repl: 100 + g()        (g not yet suppressed -> g_repl)
///   g_repl: 200 + f()        (f suppressed at shallower depth -> real f)
///   real f(): 7
///   => g_repl = 200 + 7 = 207; f_repl = 100 + 207 = 307
/// Must TERMINATE (no f<->g ping-pong). Predict 307.
#[tokio::test]
async fn torture_recursion_03_mutual_recursion_two_way_both_mocked() {
    let output = baml_test!(
        r#"
        function f() -> int { 7 }
        function g() -> int { 9 }

        function main() -> int {
            let mf = baml.mock.new(f);
            let mg = baml.mock.new(g);
            mf.replace(() -> int { 100 + g() });
            mg.replace(() -> int { 200 + f() });
            let r = 0;
            baml.mock.scope([mf, mg], () -> void {
                r = f();   // 100 + (200 + real_f=7) = 307
            });
            r
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(307)));
}

/// 3-way cycle f -> g -> h -> f, ALL mocked. Calling f():
///   f_repl: 1 + g()      (g live)
///   g_repl: 10 + h()     (h live)
///   h_repl: 100 + f()    (f suppressed at its shallower depth -> real f=2)
///   real f = 2
///   => h_repl = 100 + 2 = 102; g_repl = 10 + 102 = 112; f_repl = 1 + 112 = 113
/// Must TERMINATE. Predict 113.
#[tokio::test]
async fn torture_recursion_04_three_way_cycle_all_mocked() {
    let output = baml_test!(
        r#"
        function f() -> int { 2 }
        function g() -> int { 3 }
        function h() -> int { 4 }

        function main() -> int {
            let mf = baml.mock.new(f);
            let mg = baml.mock.new(g);
            let mh = baml.mock.new(h);
            mf.replace(() -> int { 1 + g() });
            mg.replace(() -> int { 10 + h() });
            mh.replace(() -> int { 100 + f() });
            let r = 0;
            baml.mock.scope([mf, mg, mh], () -> void {
                r = f();   // 1 + (10 + (100 + real_f=2)) = 113
            });
            r
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(113)));
}

/// A replacement opens a NEW inner scope binding a DIFFERENT mock on the SAME
/// target, THEN recurses by name — which layer is hit? The by-name call inside
/// the outer replacement happens AFTER the inner scope is active and the outer
/// mock is suppressed. So the inner mock (live, not suppressed) claims it.
///   outer.replace: open scope(inner); inside, call greeting()
///     -> outer suppressed; inner live -> inner.replace runs -> "I"
///   result "O[" + "I" + "]" = "O[I]"
/// (Mirrors mock.rs::mock_replacement_opens_inner_scope but here the inner call
/// is by-name AND the inner mock is a fresh, NON-suppressed layer — the recursion
/// must resolve to the inner mock, not skip to the real fn.)
#[tokio::test]
async fn torture_recursion_05_replacement_opens_inner_scope_then_recurses() {
    let output = baml_test!(
        r#"
        function greeting() -> string { "real" }

        function main() -> string {
            let inner = baml.mock.new(greeting);
            inner.replace(() -> string { "I" });
            let outer = baml.mock.new(greeting);
            outer.replace(() -> string {
                let cap = "";
                baml.mock.scope(inner, () -> void {
                    cap = greeting();   // outer suppressed, inner live -> "I"
                });
                "O[" + cap + "]"
            });
            let r = "";
            baml.mock.scope(outer, () -> void {
                r = greeting();   // outer's replacement -> "O[I]"
            });
            r
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::String("O[I]".into())));
}

/// A replacement recurses by name, AND the deeper call is mocked again at a
/// DEEPER scope on the same target. Outer replacement, while running, opens an
/// inner scope with a SECOND replacement, then calls by name; the inner claims it;
/// the inner ALSO recurses by name -> inner now suppressed AND outer suppressed
/// -> real fn. Composed:
///   outer: "O[" + greeting() + "]"     (opens inner scope first)
///     inner: "I[" + greeting() + "]"   (both suppressed -> real "real")
///       greeting() -> "real"
///     inner -> "I[real]"
///   outer -> "O[I[real]]"
/// Verifies the guard steps EXACTLY one down per layer across a dynamically
/// re-stacked target. Predict "O[I[real]]".
#[tokio::test]
async fn torture_recursion_06_recurse_then_deeper_remock_then_recurse() {
    let output = baml_test!(
        r#"
        function greeting() -> string { "real" }

        function main() -> string {
            let inner = baml.mock.new(greeting);
            inner.replace(() -> string { "I[" + greeting() + "]" });
            let outer = baml.mock.new(greeting);
            outer.replace(() -> string {
                let cap = "";
                baml.mock.scope(inner, () -> void {
                    cap = greeting();   // outer suppressed; inner claims, then inner recurses
                });
                "O[" + cap + "]"
            });
            let r = "";
            baml.mock.scope(outer, () -> void {
                r = greeting();
            });
            r
        }
        "#
    );
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("O[I[real]]".into()))
    );
}

/// Recursion with a SPY wedged in the middle of the chain. Stack push order
/// [a, s, c]; dispatch walks top-first (rev): c, s, a. NOTE: `seen`/spy bookkeeping
/// is PER dispatch call, so the middle spy `s` is re-counted on EVERY dispatch that
/// has to walk past it to find a lower replacement. Trace:
///   call#1 greeting() (scope body): rev -> c has replacement -> Redirect to c.
///       s NOT reached (c sits above s). s += 0.
///     c: "C[" + greeting() + "]"  (c suppressed)
///       call#2: c suppressed -> s spy (s += 1, transparent) -> a replacement -> Redirect.
///         a: "A[" + greeting() + "]"  (a & c suppressed)
///           call#3: c suppressed -> s spy (s += 1) -> a suppressed -> only spy -> real "real".
///         a -> "A[real]"
///     c -> "C[A[real]]"
///   Final r = "C[A[real]]" (len 10); s.call_count = 2 (counted on call#2 AND call#3).
/// Encode: r.length()*10 + s.call_count -> 10*10 + 2 = 102.
#[tokio::test]
async fn torture_recursion_07_recursion_with_spy_in_middle() {
    let output = baml_test!(
        r#"
        function greeting() -> string { "real" }

        function main() -> int {
            let a = baml.mock.new(greeting);
            a.replace(() -> string { "A[" + greeting() + "]" });
            let s = baml.mock.new(greeting);          // pure spy in the middle
            let c = baml.mock.new(greeting);
            c.replace(() -> string { "C[" + greeting() + "]" });
            let r = "";
            baml.mock.scope(a, () -> void {
                baml.mock.scope(s, () -> void {
                    baml.mock.scope(c, () -> void {
                        r = greeting();   // C[A[real]]
                    });
                });
            });
            r.length() * 10 + s.call_count   // 10*10 + 2 = 102
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(102)));
}

/// A replacement calls the target by name inside a try/catch. The by-name call
/// steps down to the real fn, which THROWS; the catch in the replacement swallows
/// it and substitutes a value. The guard must not corrupt: the suppress entry was
/// pushed at the replacement's call depth and the inner throw unwinds back to the
/// catch (same replacement frame), then control resumes. A LATER by-name call in
/// the same replacement (after the catch) must STILL be suppressed -> real fn
/// again. real f throws "boom".
///   repl: try { real f -> throw } catch -> 50; then call f() again -> real throws
///         again -> NOT caught this time? It IS: wrap second in its own behavior.
/// Keep it simple: one guarded call inside try, return 50 + (a second guarded call
/// that returns the real non-throwing value). Use a throwing real fn only in the
/// try. Predict 50 + 9 = 59.
#[tokio::test]
async fn torture_recursion_08_recurse_into_throwing_real_inside_try() {
    let output = baml_test!(
        r#"
        function boomy(flag: int) -> int throws string {
            if flag == 1 { throw "boom" } else { 9 }
        }

        function main() -> int {
            let m = baml.mock.new(boomy);
            m.replace((flag: int) -> int throws string {
                let caught = 0;
                let _ = boomy(1) catch (e) {     // real fn (suppressed) throws -> caught
                    _ => { caught = 50; 0 }
                };
                let ok = boomy(0);               // suppressed again -> real -> 9
                caught + ok                      // 50 + 9 = 59
            });
            let r = 0;
            baml.mock.scope(m, () -> void {
                r = boomy(0);   // enters replacement; flag arg ignored inside
            });
            r
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(59)));
}

/// A replacement that recurses by name and THROWS out of the deepest real frame,
/// with NO catch in the replacement: the throw propagates as the call's, the
/// scope unwinds (finally-style deactivation), and the program's outer catch sees
/// it. Crucially, after the (caught) scope, the mock is gone AND the suppress
/// stack must be clean (no leaked suppression that would mis-route a later call).
/// After catch: call boomy(0) outside scope -> real -> 9. caught flag 7000.
/// Predict 7000 + 9 = 7009.
#[tokio::test]
async fn torture_recursion_09_recurse_throw_unwinds_scope_and_guard() {
    let output = baml_test!(
        r#"
        function boomy(flag: int) -> int throws string {
            if flag == 1 { throw "boom" } else { 9 }
        }

        function main() -> int {
            let m = baml.mock.new(boomy);
            m.replace((flag: int) -> int throws string { boomy(1) });   // recurse -> real throws
            let caught = 0;
            baml.mock.scope(m, () -> void {
                let _ = boomy(0);   // enters replacement -> real boomy(1) throws -> unwinds
            }) catch (e) {
                _ => { caught = 7000 }
            };
            let after = boomy(0);   // scope popped, suppress clean -> real -> 9
            caught + after          // 7000 + 9 = 7009
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(7009)));
}

/// By-name re-entry inside a real recursive fn vs. a separate by-name probe in
/// the replacement. The replacement makes its OWN by-name call AND the real fn it
/// reaches is itself recursive (every internal self-call also by-name). Every one
/// of those re-entries must step down to the real fn — none re-enters the head.
///   real f(n) = n + f(n-1) for n>0, 0 at n<=0. repl: f(3) by name -> real chain.
///   real f(3) = 3+2+1+0 = 6. repl returns f(3) = 6.
/// call_count: the head fired exactly ONCE (the outer call). All the recursive
/// descents land in the real fn, not the head. Encode result*10 + m.call_count.
/// A guard that mis-prunes would either re-enter the head (count > 1 / hang) or
/// skip a layer. Predict 6*10 + 1 = 61. could_hang on a guard prune bug.
#[tokio::test]
async fn torture_recursion_10_byname_into_real_recursion_head_fires_once() {
    let output = baml_test!(
        r#"
        function f(n: int) -> int {
            if n <= 0 { 0 } else { n + f(n - 1) }
        }

        function main() -> int {
            let m = baml.mock.new(f);
            m.replace((n: int) -> int { f(3) });   // by-name -> real recursive chain
            let r = 0;
            baml.mock.scope(m, () -> void {
                r = f(99);   // arg ignored; replacement computes real f(3) = 6
            });
            r * 10 + m.call_count   // 60 + 1 = 61
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(61)));
}

/// Genuine BOUNDED self-recursion routed through the real fn: the replacement
/// adds a marker, then calls the real recursive fn which decrements and calls
/// ITSELF by name many times (every one of those re-entries is suppressed -> stays
/// in the real fn, so the recursion actually terminates at n<=0). Depth ~30.
///   real countdown(30) = 30+29+...+1+0 = 465. repl = 10000 + 465 = 10465.
/// Stresses that a long real recursion under an active mock never bounces back up
/// to the replacement (which would blow the stack / loop). Predict 10465.
#[tokio::test]
async fn torture_recursion_11_deep_real_recursion_under_active_mock() {
    let output = baml_test!(
        r#"
        function countdown(n: int) -> int {
            if n <= 0 { 0 } else { n + countdown(n - 1) }
        }

        function main() -> int {
            let m = baml.mock.new(countdown);
            m.replace((n: int) -> int { 10000 + countdown(n) });
            let r = 0;
            baml.mock.scope(m, () -> void {
                r = countdown(30);   // 10000 + 465 = 10465
            });
            r
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(10465)));
}

/// Stacked replacements where EACH layer recurses by name, 4 deep: a,b,c,d each
/// wrap "X[" + greeting() + "]". By the guard, each by-name call steps exactly one
/// down, so the composed result threads all four then the real fn ONCE:
///   d -> "d[" + c -> "c[" + b -> "b[" + a -> "a[" + real "real" + "]" ...
///   = "d[c[b[a[real]]]]"
/// This is the super-chain expressed purely through by-name re-entry. Any
/// off-by-one in the suppress-depth pruning would either skip a layer (fewer
/// brackets) or loop. Predict "d[c[b[a[real]]]]" (len 16). Encode via length: 16.
#[tokio::test]
async fn torture_recursion_12_four_deep_byname_chain_threads_each_once() {
    let output = baml_test!(
        r#"
        function greeting() -> string { "real" }

        function main() -> int {
            let a = baml.mock.new(greeting); a.replace(() -> string { "a[" + greeting() + "]" });
            let b = baml.mock.new(greeting); b.replace(() -> string { "b[" + greeting() + "]" });
            let c = baml.mock.new(greeting); c.replace(() -> string { "c[" + greeting() + "]" });
            let d = baml.mock.new(greeting); d.replace(() -> string { "d[" + greeting() + "]" });
            let r = "";
            baml.mock.scope([a, b, c, d], () -> void {
                r = greeting();   // d[c[b[a[real]]]]
            });
            r.length()   // 16
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(16)));
}

/// INSTANCE-mock recursion with strip_self: an instance mock on `a.bump` whose
/// receiver-less replacement recurses by calling `a.bump()` BY NAME. The instance
/// key (Instance(recv, "bump"), strip_self=true) must be suppressed during the
/// replacement so the by-name `a.bump()` steps one down to the REAL method (which
/// takes self), not back into the receiver-less replacement. A strip_self/suppress
/// interaction bug would either re-enter the head (the receiver-less replacement
/// gets a self it cannot take -> arity error / loop) or drop the receiver wrongly.
///   real a.bump() = a.count + 1 = 0 + 1 = 1. repl: 500 + a.bump() = 500 + 1 = 501.
/// call_count: head fired once (the outer a.bump()); the inner by-name reaches the
/// real method. Encode 501 + m.call_count*10000 -> 501 + 10000 = 10501.
/// could_hang if the instance-key suppress fails to step down.
#[tokio::test]
async fn torture_recursion_13_instance_mock_byname_recursion_strip_self() {
    let output = baml_test!(
        r#"
        class Counter {
          count int
          function bump(self) -> int { self.count + 1 }
        }

        function main() -> int {
            let a = Counter { count: 0 };
            let m = baml.mock.new(a.bump);          // instance mock, strip_self
            m.replace(() -> int { 500 + a.bump() }); // receiver-less; recurses by name
            let r = 0;
            baml.mock.scope(m, () -> void {
                r = a.bump();   // repl: 500 + real a.bump()=1 = 501
            });
            r + m.call_count * 10000   // 501 + 10000 = 10501
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(10501)));
}

/// Recursion across a spawn by NAME: the replacement spawns a task that calls
/// f() by name. The recursion guard crosses the spawn boundary (see
/// torture_spawn_06): the child inherits the parent's suppression of the running
/// replacement, so the child's by-name call steps one level down to the real fn
/// rather than re-entering the replacement. The guard therefore CUTS cross-spawn
/// recursion at the first spawn — `f(n - 1)` in the child resolves to real
/// regardless of `n`.
///   real f(n): n. repl(n): if n <= 0 { f(n) } else { await spawn { f(n - 1) } } + 1
/// Trace n=1: repl(1): n>0 -> spawn { f(0) } in CHILD (inherits suppress of the
///   running repl) -> child by-name f(0) -> suppressed -> real f(0)=0.
///   await -> 0. repl(1) returns 0 + 1 = 1.
/// Predict 1.
#[tokio::test]
async fn torture_recursion_14_byname_recursion_across_spawn_bounded() {
    let output = baml_test!(
        r#"
        function f(n: int) -> int { n }

        function main() -> int {
            let m = baml.mock.new(f);
            m.replace((n: int) -> int {
                let res = 0;
                if n <= 0 {
                    res = f(n);                    // suppressed-in-parent path -> real 0
                } else {
                    let fut = spawn { f(n - 1) };  // child inherits suppress -> real f(n-1)
                    res = await fut;
                }
                res + 1
            });
            let r = 0;
            baml.mock.scope(m, () -> void {
                r = f(1);   // repl(1): spawn f(0) -> child real 0 -> await 0 -> +1 = 1
            });
            r
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(1)));
}

/// Pure-spy recursion guard: a spy (no replacement) on a real recursive fn. Spies
/// are transparent — every real recursive self-call is observed (counted) AND runs
/// the real fn. So call_count = number of real invocations, and the result is the
/// true recursive value. The guard must NOT suppress a spy's recursive descents
/// (a spy has no replacement to recurse into; it just delegates down each time).
///   real fib-ish: walk(3) calls walk(2)+walk(1); walk(2)->walk(1)+walk(0)...
///   count nodes. walk(n)= 1 if n<2 else walk(n-1)+walk(n-2). walk(4)=5 (=fib(5)).
///   Number of walk() invocations for walk(4): nodes of the call tree = 9.
/// Encode: result*100 + call_count -> 5*100 + 9 = 509.
#[tokio::test]
async fn torture_recursion_15_pure_spy_counts_every_recursive_call() {
    let output = baml_test!(
        r#"
        function walk(n: int) -> int {
            if n < 2 { 1 } else { walk(n - 1) + walk(n - 2) }
        }

        function main() -> int {
            let m = baml.mock.new(walk);   // pure spy, no .replace
            let r = 0;
            baml.mock.scope(m, () -> void {
                r = walk(4);   // real fib value 5; every node observed
            });
            r * 100 + m.call_count   // 5*100 + 9 = 509
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(509)));
}

/// Adversarial suppress-depth pruning: a replacement recurses by name into the
/// real fn, which RETURNS, and THEN the same replacement makes a SECOND by-name
/// call at the SAME frame depth as the first. The suppress entry for the head was
/// pushed at the replacement's call depth; both inner by-name calls happen strictly
/// deeper, so BOTH must be suppressed -> both reach the real fn (not the head).
/// If the prune (`depth > stored`) wrongly dropped the entry after the first
/// inner call returned, the second by-name call would re-enter the head -> infinite
/// loop. real f(x)=x. repl: f(10) + f(20) = 10 + 20 = 30 (each a single real hit).
/// call_count: head fired once. Encode 30*10 + m.call_count = 300 + 1 = 301.
/// could_hang if the second by-name call re-triggers the head.
#[tokio::test]
async fn torture_recursion_16_two_sequential_byname_calls_both_suppressed() {
    let output = baml_test!(
        r#"
        function f(x: int) -> int { x }

        function main() -> int {
            let m = baml.mock.new(f);
            m.replace((x: int) -> int { f(10) + f(20) });   // two sequential by-name calls
            let r = 0;
            baml.mock.scope(m, () -> void {
                r = f(0);   // 10 + 20 = 30, both reach real f
            });
            r * 10 + m.call_count   // 300 + 1 = 301
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(301)));
}

/// Nested by-name calls (one inside the args of another) under a single mock: the
/// outer by-name f(...) is suppressed, and its ARGUMENT is itself a by-name f(...)
/// evaluated at an even deeper expression depth — also suppressed. Both reach the
/// real fn; neither re-enters the head. real f(x)=x+1.
///   repl: f(f(5)) -> inner f(5)=real 6; outer f(6)=real 7. -> 7
/// call_count: head once. Encode 7*10 + 1 = 71.
#[tokio::test]
async fn torture_recursion_17_nested_byname_in_argument_position() {
    let output = baml_test!(
        r#"
        function f(x: int) -> int { x + 1 }

        function main() -> int {
            let m = baml.mock.new(f);
            m.replace((x: int) -> int { f(f(5)) });   // nested by-name in arg
            let r = 0;
            baml.mock.scope(m, () -> void {
                r = f(0);   // f(f(5)) -> f(6)=7, both real
            });
            r * 10 + m.call_count   // 70 + 1 = 71
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(71)));
}

/// Mutual recursion where the second target is mocked AGAIN at a deeper scope.
/// g has an OUTER mock mg1 (active for the whole body) and an INNER mock mg2 opened
/// inside f's replacement. So inside that inner scope BOTH g-mocks are active
/// (g-key stack [mg1, mg2]); only mg2 is suppressed when it recurses by name, so
/// the step-down lands on mg1 (NOT the real fn). Trace:
///   f_repl: open scope(mg2); call g()  -> rev: mg2 (top) replacement -> Redirect.
///     mg2_repl: "g2[" + g() + "]"  (mg2 suppressed)
///       g(): mg2 suppressed -> mg1 live -> Redirect.
///         mg1_repl: "g1[" + g() + "]"  (mg1 & mg2 suppressed)
///           g(): both suppressed -> real g "G"
///         mg1_repl -> "g1[G]"
///       mg2_repl -> "g2[g1[G]]"
///   f_repl -> "F[g2[g1[G]]]"
/// Stresses re-stacking the SECOND target mid-recursion of the FIRST, where the
/// step-down must reach the OUTER mock, not skip to the real fn. Predict
/// "F[g2[g1[G]]]" (len 12).
#[tokio::test]
async fn torture_recursion_18_mutual_with_deeper_remock_of_second_target() {
    let output = baml_test!(
        r#"
        function f() -> string { "f" }
        function g() -> string { "G" }

        function main() -> string {
            let mg1 = baml.mock.new(g);
            mg1.replace(() -> string { "g1[" + g() + "]" });
            let mg2 = baml.mock.new(g);
            mg2.replace(() -> string { "g2[" + g() + "]" });
            let mf = baml.mock.new(f);
            mf.replace(() -> string {
                let cap = "";
                baml.mock.scope(mg2, () -> void {
                    cap = g();   // mg2 innermost; recurses -> mg1 (outer, live) -> real G
                });
                "F[" + cap + "]"
            });
            let r = "";
            baml.mock.scope([mg1, mf], () -> void {
                r = f();   // f_repl opens mg2 scope -> "F[g2[g1[G]]]"
            });
            r
        }
        "#
    );
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("F[g2[g1[G]]]".into()))
    );
}
