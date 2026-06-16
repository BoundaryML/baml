//! BEP-058 function mocking — SPAWN TORTURE pass.
//!
//! Extreme / sad-path stress of mock propagation across the `spawn` boundary:
//! deep nested spawns, many concurrent spawns racing the shared atomic
//! `call_count`, spawns owning their own scope, spawns inside replacements and
//! spies, by-name re-entry inside a child VM (fresh `mock_suppress`), throwing
//! spawns, detached spawns awaited across scopes, isolation, and instance mocks
//! whose receiver crosses the parent->child VM boundary under heavy alloc.
//!
//! Predictions trace the BEP-058 semantics (the source of truth), NOT an
//! assumption that the implementation is correct — the point is to surface
//! divergences, hangs, and corruption.

use baml_tests::baml_test;
use bex_engine::BexExternalValue;

/// Five-deep nested spawns each calling the mocked target: the snapshot
/// propagates down every level and the shared atomic call_count sums all 5.
#[tokio::test]
async fn torture_spawn_01_five_deep_nested_spawns_sum_call_count() {
    let output = baml_test!(
        r#"
        function target() -> int { 1 }

        function main() -> int {
            let m = baml.mock.new(target);
            m.replace(() -> int { 2 });
            let r = 0;
            baml.mock.scope(m, () -> void {
                let f = spawn {
                    let g = spawn {
                        let h = spawn {
                            let i = spawn {
                                let j = spawn { target() };   // depth 5
                                target() + (await j)
                            };
                            target() + (await i)
                        };
                        target() + (await h)
                    };
                    target() + (await g)
                };
                r = await f;                                  // 2*5 = 10
            });
            r * 100 + m.call_count                            // 10*100 + 5 = 1005
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(1005)));
}

/// Sixteen concurrent spawns race the single shared atomic call_count; awaiting
/// all of them, the count must be exactly 16 with no lost increments.
#[tokio::test]
async fn torture_spawn_02_sixteen_concurrent_spawns_exact_count() {
    let output = baml_test!(
        r#"
        function target() -> int { 1 }

        function main() -> int {
            let m = baml.mock.new(target);
            m.replace(() -> int { 1 });
            let total = 0;
            baml.mock.scope(m, () -> void {
                let f0 = spawn { target() };  let f1 = spawn { target() };
                let f2 = spawn { target() };  let f3 = spawn { target() };
                let f4 = spawn { target() };  let f5 = spawn { target() };
                let f6 = spawn { target() };  let f7 = spawn { target() };
                let f8 = spawn { target() };  let f9 = spawn { target() };
                let fa = spawn { target() };  let fb = spawn { target() };
                let fc = spawn { target() };  let fd = spawn { target() };
                let fe = spawn { target() };  let ff = spawn { target() };
                total = (await f0) + (await f1) + (await f2) + (await f3)
                      + (await f4) + (await f5) + (await f6) + (await f7)
                      + (await f8) + (await f9) + (await fa) + (await fb)
                      + (await fc) + (await fd) + (await fe) + (await ff);
            });
            total * 1000 + m.call_count                       // 16*1000 + 16 = 16016
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(16016)));
}

/// A spawn creates and scopes its OWN mock, independent of the parent snapshot:
/// the parent never sees the child's mock, the child sees only its own.
#[tokio::test]
async fn torture_spawn_03_spawn_owns_independent_scope() {
    let output = baml_test!(
        r#"
        function target() -> int { 1 }

        function main() -> int {
            let parent = 0;
            let child = 0;
            let f = spawn {
                let cm = baml.mock.new(target);   // child's own mock, parent unaware
                cm.replace(() -> int { 50 });
                let inner = 0;
                baml.mock.scope(cm, () -> void {
                    inner = target();             // child mock -> 50
                });
                inner * 10 + cm.call_count        // 50*10 + 1 = 501
            };
            child = await f;
            parent = target();                    // parent never saw cm -> real 1
            child + parent                        // 501 + 1 = 502
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(502)));
}

/// A spawn launched from INSIDE a replacement body. The replacement runs under
/// suppression in the parent, but the spawned child VM starts with empty
/// mock_suppress and its own snapshot, so target() inside the spawn re-enters
/// the (now visible, unsuppressed) mock and hits the replacement again. This
/// recurses by re-spawning unless the spawn calls something else; here it calls
/// a DISTINCT helper so it terminates deterministically.
#[tokio::test]
async fn torture_spawn_04_spawn_inside_replacement_body() {
    let output = baml_test!(
        r#"
        function target() -> int { 1 }
        function helper() -> int { 7 }

        function main() -> int {
            let m = baml.mock.new(target);
            m.replace(() -> int {
                let f = spawn { helper() };   // spawn from within the replacement
                3 + (await f)                 // 3 + 7 = 10
            });
            let r = 0;
            baml.mock.scope(m, () -> void {
                r = target();                 // replacement runs, spawns, -> 10
            });
            r * 10 + m.call_count             // 10*10 + 1 = 101
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(101)));
}

/// A spawn launched from inside a pure SPY's observed call path. The spy is
/// transparent: the original runs, the spy counts. Inside the original body we
/// spawn a call to a SECOND mocked target, which the snapshot also carries.
#[tokio::test]
async fn torture_spawn_05_spawn_inside_spy_original() {
    let output = baml_test!(
        r#"
        function outer() -> int { let f = spawn { inner() }; 4 + (await f) }
        function inner() -> int { 1 }

        function main() -> int {
            let so = baml.mock.new(outer);       // pure spy on outer (runs original)
            let mi = baml.mock.new(inner);
            mi.replace(() -> int { 20 });        // inner mocked -> 20
            let r = 0;
            baml.mock.scope([so, mi], () -> void {
                r = outer();   // spy runs real outer, which spawns inner() -> 20; 4+20=24
            });
            r * 100 + so.call_count * 10 + mi.call_count  // 24*100 + 1*10 + 1 = 2411
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(2411)));
}

/// RECURSION GUARD IN A CHILD VM (fresh suppress). A replacement calls the
/// target by name; that delegation is spawned. In the parent the guard
/// suppresses the running mock so the by-name call steps down to the real fn.
/// But the spawned child starts with EMPTY mock_suppress and a snapshot that
/// still contains the mock — so target() in the child re-enters the SAME
/// replacement, which spawns again, re-entering again: unbounded re-spawn.
/// Per the BEP recursion-guard intent this SHOULD terminate (by-name re-entry
/// steps one down to the real fn), so the predicted value assumes the guard
/// holds across the boundary. If suppression does not carry, this HANGS.
// IGNORED: aspirational slice-9 gap — recursion-guard suppression does not yet
// carry into spawned children, so this infinite-loops. UN-IGNORE once the
// spawn recursion-guard-across-spawn fix lands.
#[ignore = "spawn recursion-guard-across-spawn not implemented (BEP-058 slice 9 gap); infinite-loops"]
#[tokio::test]
async fn torture_spawn_06_recursion_guard_in_child_via_spawned_delegation() {
    let output = baml_test!(
        r#"
        function f(x: int) -> int { x }

        function main() -> int {
            let m = baml.mock.new(f);
            m.replace((x: int) -> int {
                let s = spawn { f(x) };   // by-name re-entry, but in a child VM
                (await s) + 1
            });
            let r = 0;
            baml.mock.scope(m, () -> void {
                r = f(5);                 // guard: inner f(5) -> real 5, +1 -> 6
            });
            r
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(6)));
}

/// A spawn that recurses BY NAME directly in its own body under an active mock.
/// The child VM has fresh suppress, so the by-name call inside the replacement
/// (reached via the spawned target) must still step down to the real fn. With
/// the guard, it terminates; without it, infinite recursion in the child.
#[tokio::test]
async fn torture_spawn_07_spawn_body_calls_mocked_target_recursing_replacement() {
    let output = baml_test!(
        r#"
        function f(x: int) -> int { x * 3 }

        function main() -> int {
            let m = baml.mock.new(f);
            m.replace((x: int) -> int { f(x) + 100 });  // calls original once (guard)
            let r = 0;
            baml.mock.scope(m, () -> void {
                let s = spawn { f(2) };   // in child: replacement, inner f(2)->6, +100
                r = await s;              // 106
            });
            r * 10 + m.call_count         // 106*10 + 1 = 1061
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(1061)));
}

/// A spawn THROWS while a mock is active. Awaiting it re-throws into the parent;
/// the catch runs, the scope unwinds (mock popped), and a post-scope call hits
/// the real function — mock state intact after the faulting spawn.
#[tokio::test]
async fn torture_spawn_08_spawn_throws_while_mock_active() {
    let output = baml_test!(
        r#"
        function target() -> int { 1 }

        function main() -> int {
            let m = baml.mock.new(target);
            m.replace(() -> int { 99 });
            let caught = 0;
            baml.mock.scope(m, () -> void {
                let s = spawn { let _ = target(); throw "boom" };
                (await s) catch (e) {
                    _ => { caught = 1 }
                };
            });
            let after = target();             // scope popped -> real 1
            caught * 1000 + after             // 1000 + 1 = 1001
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(1001)));
}

/// A faulting spawned mocked call still counts toward call_count (BEP Errors:
/// "a faulting mocked call still counts"). The replacement throws; awaiting
/// re-throws; the call_count recorded the call before the throw.
#[tokio::test]
async fn torture_spawn_09_faulting_spawned_call_still_counted() {
    let output = baml_test!(
        r#"
        function target() -> int throws string { 1 }

        function main() -> int {
            let m = baml.mock.new(target);
            m.replace(() -> int throws string { throw "nope" });
            let caught = 0;
            baml.mock.scope(m, () -> void {
                let s = spawn { target() };
                (await s) catch (e) {
                    _ => { caught = 1 }
                };
            });
            caught * 10 + m.call_count        // call counted before throw: 10 + 1 = 11
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(11)));
}

/// DETACHED spawn awaited after the scope returned AND after re-entering a
/// DIFFERENT scope: the detached spawn's snapshot (mock a) is independent of
/// both the parent's __exit and the later scope (mock b). Awaiting it last must
/// still hit a's replacement, not b's, not the real fn.
#[tokio::test]
async fn torture_spawn_10_detached_spawn_awaited_across_a_different_scope() {
    let output = baml_test!(
        r#"
        function greeting() -> string { "real" }

        function main() -> string {
            let a = baml.mock.new(greeting);
            a.replace(() -> string { "A" });
            let b = baml.mock.new(greeting);
            b.replace(() -> string { "B" });
            let held = spawn { "x" };               // placeholder
            baml.mock.scope(a, () -> void {
                held = spawn { greeting() };        // snapshot = [a]
            });
            let mid = "";
            baml.mock.scope(b, () -> void {
                mid = greeting();                   // "B" (b active)
            });
            let after = await held;                 // still "A" (own snapshot)
            mid + after                             // "BA"
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::String("BA".into())));
}

/// ISOLATION: a spawn created OUTSIDE any active scope captures an empty
/// snapshot; it never sees a mock activated later in the parent, even when the
/// parent activates it before the spawn is awaited.
#[tokio::test]
async fn torture_spawn_11_outside_spawn_blind_to_later_scope() {
    let output = baml_test!(
        r#"
        function target() -> int { 1 }

        function main() -> int {
            let m = baml.mock.new(target);
            m.replace(() -> int { 99 });
            let outside = spawn { target() };    // snapshot empty (no scope yet)
            let inside = 0;
            baml.mock.scope(m, () -> void {
                inside = await outside;          // awaited inside, but its snapshot is empty -> 1
            });
            inside * 100 + m.call_count          // 100 + 0 (spawn never hit the mock) = 100
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(100)));
}

/// INSTANCE mock crossing the VM boundary under heavy allocation. The receiver
/// pointer keys the Instance mock; the spawn shares it via the heap. Heavy
/// allocation in the child (forcing GC / promotion) must not break the
/// receiver-keyed lookup: the relocated receiver stays matched (collect_roots /
/// forward_roots re-key the table), and a different instance is unaffected.
#[tokio::test]
async fn torture_spawn_12_instance_mock_across_boundary_heavy_alloc() {
    let output = baml_test!(
        r#"
        class Counter {
          count int
          function bump(self) -> int { self.count + 1 }
        }

        function churn(n: int) -> int {
            let acc = 0;
            let i = 0;
            while (i < n) {
                let s = "alloc" + "ation" + "garbage";   // young allocations
                acc = acc + s.length();
                i = i + 1;
            }
            acc
        }

        function main() -> int {
            let a = Counter { count: 0 };
            let b = Counter { count: 10 };
            let m = baml.mock.new(a.bump);    // instance mock on `a` only
            m.replace(() -> int { -1 });
            let ra = 0;
            let rb = 0;
            baml.mock.scope(m, () -> void {
                let fa = spawn { let _ = churn(200); a.bump() };  // mocked -> -1
                let fb = spawn { let _ = churn(200); b.bump() };  // real -> 11
                ra = await fa;
                rb = await fb;
            });
            ra * 100 + rb + m.call_count       // -1*100 + 11 + 1 = -88
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(-88)));
}

/// Stacked mocks + a spawn at the BOTTOM of the stack that delegates by name.
/// Inside the scope [a, b] (b innermost), a spawn calls greeting(). In the
/// child the snapshot is [a, b]; the innermost replacement b runs, its
/// b.super-style by-name call steps down to a, whose by-name steps to real.
/// Tests the full delegation chain reconstructed in a fresh child VM.
#[tokio::test]
async fn torture_spawn_13_stacked_delegation_chain_reconstructed_in_child() {
    let output = baml_test!(
        r#"
        function greeting() -> string { "hello" }

        function main() -> string {
            let a = baml.mock.new(greeting);
            a.replace(() -> string { "A[" + greeting() + "]" });
            let b = baml.mock.new(greeting);
            b.replace(() -> string { "B[" + greeting() + "]" });
            let r = "";
            baml.mock.scope([a, b], () -> void {
                let f = spawn { greeting() };   // child rebuilds chain: B[A[hello]]
                r = await f;
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

/// Generic-specialization mock crossing the spawn boundary: only identity<int>
/// is mocked. In a child, identity<int> hits the spec; identity<string> falls
/// through to the real fn. The Generic key (name + type_args) must survive the
/// mock_table clone into the child and re-match on the baked type args.
#[tokio::test]
async fn torture_spawn_14_generic_specialization_across_boundary() {
    let output = baml_test!(
        r#"
        function identity<T>(x: T) -> T { x }

        function main() -> int {
            let m = baml.mock.new(identity<int>);
            m.replace((x: int) -> int { -7 });
            let ri = 0;
            let rs = 0;
            baml.mock.scope(m, () -> void {
                let fi = spawn { identity<int>(5) };          // spec -> -7
                let fs = spawn { identity<string>("hi").length() };  // real -> 2
                ri = await fi;
                rs = await fs;
            });
            ri * 100 + rs + m.call_count    // -7*100 + 2 + 1 = -697
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(-697)));
}

/// Many concurrent spawns that EACH nest more spawns (fan-out * depth), all
/// hitting one shared mock. 4 outer spawns each spawn 3 inner = 16 total calls.
/// Stresses the atomic under a wider concurrent fan-out than a flat batch.
#[tokio::test]
async fn torture_spawn_15_fanout_times_depth_shared_count() {
    let output = baml_test!(
        r#"
        function target() -> int { 1 }

        function main() -> int {
            let m = baml.mock.new(target);
            m.replace(() -> int { 1 });
            let total = 0;
            baml.mock.scope(m, () -> void {
                let o0 = spawn {
                    let i0 = spawn { target() };
                    let i1 = spawn { target() };
                    let i2 = spawn { target() };
                    target() + (await i0) + (await i1) + (await i2)   // 4
                };
                let o1 = spawn {
                    let i0 = spawn { target() };
                    let i1 = spawn { target() };
                    let i2 = spawn { target() };
                    target() + (await i0) + (await i1) + (await i2)   // 4
                };
                let o2 = spawn {
                    let i0 = spawn { target() };
                    let i1 = spawn { target() };
                    let i2 = spawn { target() };
                    target() + (await i0) + (await i1) + (await i2)   // 4
                };
                let o3 = spawn {
                    let i0 = spawn { target() };
                    let i1 = spawn { target() };
                    let i2 = spawn { target() };
                    target() + (await i0) + (await i1) + (await i2)   // 4
                };
                total = (await o0) + (await o1) + (await o2) + (await o3);  // 16
            });
            total * 100 + m.call_count       // 16*100 + 16 = 1616
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(1616)));
}

/// A spawn created inside an INNER scope is awaited after the inner scope exits
/// but while the OUTER scope of a stacked pair is still active. Its snapshot is
/// [a, b]; even though b's scope has lexically ended in the parent, the
/// detached child keeps both active, so it resolves to b's replacement.
#[tokio::test]
async fn torture_spawn_16_inner_scope_spawn_outlives_inner_exit() {
    let output = baml_test!(
        r#"
        function greeting() -> string { "real" }

        function main() -> string {
            let a = baml.mock.new(greeting);
            a.replace(() -> string { "A" });
            let b = baml.mock.new(greeting);
            b.replace(() -> string { "B" });
            let held = spawn { "x" };
            let mid = "";
            baml.mock.scope(a, () -> void {
                baml.mock.scope(b, () -> void {
                    held = spawn { greeting() };    // snapshot = [a, b], b innermost
                });
                mid = greeting();                   // b popped in parent -> "A"
            });
            let after = await held;                 // child still has b -> "B"
            mid + after                             // "AB"
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::String("AB".into())));
}

/// A pure SPY (no replacement) crossing into a spawn: the spawned call runs the
/// original and bumps the shared count. Awaited after the scope, the spawn's
/// own snapshot keeps the spy active so the count still increments for it.
#[tokio::test]
async fn torture_spawn_17_spy_count_via_detached_spawn() {
    let output = baml_test!(
        r#"
        function target() -> int { 5 }

        function main() -> int {
            let m = baml.mock.new(target);   // pure spy, no replace
            let held = spawn { 0 };
            baml.mock.scope(m, () -> void {
                held = spawn { target() };   // spy snapshot, runs real -> 5, counted
            });
            let r = await held;              // 5
            r * 10 + m.call_count            // 5*10 + 1 = 51
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(51)));
}

/// Two DIFFERENT instances each mocked, both crossing into separate spawns at
/// once. Each Instance key must remain distinct across the boundary; no
/// cross-fire even when both receivers are alive in concurrent children.
#[tokio::test]
async fn torture_spawn_18_two_instance_mocks_distinct_across_spawns() {
    let output = baml_test!(
        r#"
        class Counter {
          count int
          function bump(self) -> int { self.count + 1 }
        }

        function main() -> int {
            let a = Counter { count: 0 };
            let b = Counter { count: 0 };
            let ma = baml.mock.new(a.bump);
            ma.replace(() -> int { 7 });
            let mb = baml.mock.new(b.bump);
            mb.replace(() -> int { 9 });
            let ra = 0;
            let rb = 0;
            baml.mock.scope([ma, mb], () -> void {
                let fa = spawn { a.bump() };   // a's mock -> 7
                let fb = spawn { b.bump() };   // b's mock -> 9
                ra = await fa;
                rb = await fb;
            });
            ra * 100 + rb * 10 + ma.call_count + mb.call_count  // 700 + 90 + 1 + 1 = 792
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(792)));
}
