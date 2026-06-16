//! BEP-058 function mocking — TORTURE: precedence conflicts.
//!
//! These push the precedence walk in `mock_dispatch` to its breaking point:
//! one call matching several keys at once (Instance / Free-class / Free-interface,
//! and Generic / Free), 5-8 mocks stacked on one key in adversarial spy/replace
//! orders, the same Mock object scoped twice and also placed in an array,
//! re-replacing mid-scope, replacements that are themselves mocked, and deep
//! transparent spy-over-spy-over-replacement chains. Each result is predicted by
//! tracing the BEP semantics (candidates: Generic -> Instance -> Free -> interface
//! key; top-of-stack first; seen-dedup; first replacement wins; spies transparent;
//! suppression steps one down on re-entry).

use baml_tests::baml_test;
use bex_engine::BexExternalValue;

/// One interface-method call matches THREE keys at once: an instance mock on the
/// receiver, a class (Free) mock, and an interface (Free, interface-derived) mock.
/// Precedence walk order is Instance -> Free(class) -> Free(interface); the first
/// replacement claims it. Instance has a replacement -> wins. Class and interface
/// mocks below it are NOT reached (walk stops at first replacement), so only the
/// instance mock's call_count bumps.
#[tokio::test]
async fn torture_precedence_conflict_01_instance_beats_class_beats_interface() {
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

        function main() -> int {
            let d = Dog {};
            let im = baml.mock.new(d.speak);            // Instance key
            im.replace(() -> string { "I" });
            let cm = baml.mock.new(Dog.speak);          // Free (class) key
            cm.replace((self: Dog) -> string { "C" });
            let am = baml.mock.new(Animal.speak);       // Free (interface) key
            am.replace((self: Animal) -> string { "A" });

            let r = "";
            baml.mock.scope(cm, () -> void {
                baml.mock.scope(am, () -> void {
                    baml.mock.scope(im, () -> void {
                        r = d.speak();                  // instance wins -> "I"
                    });
                });
            });
            // r len 1; only instance counted (1), class 0, interface 0.
            r.length() * 1000 + im.call_count * 100 + cm.call_count * 10 + am.call_count
            // 1*1000 + 1*100 + 0 + 0 = 1100
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(1100)));
}

/// Same triple-key call, but the instance and class mocks are PURE SPIES and only
/// the interface mock has a replacement. The walk reaches instance spy (count++,
/// transparent), class spy (count++, transparent), then the interface replacement
/// claims it. All three call_counts bump; the result is the interface stand-in.
#[tokio::test]
async fn torture_precedence_conflict_02_spies_above_interface_replacement_all_count() {
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

        function main() -> int {
            let d = Dog {};
            let im = baml.mock.new(d.speak);          // instance spy (no replace)
            let cm = baml.mock.new(Dog.speak);        // class spy (no replace)
            let am = baml.mock.new(Animal.speak);     // interface replacement
            am.replace((self: Animal) -> string { "muted" });

            let r = "";
            baml.mock.scope(cm, () -> void {
                baml.mock.scope(im, () -> void {
                    baml.mock.scope(am, () -> void {
                        r = d.speak();                // instance spy + class spy transparent -> "muted"
                    });
                });
            });
            // "muted" len 5; every reached mock counted: im 1, cm 1, am 1.
            r.length() * 1000 + im.call_count * 100 + cm.call_count * 10 + am.call_count
            // 5*1000 + 1*100 + 1*10 + 1 = 5111
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(5111)));
}

/// Eight mocks on ONE Free key, adversarial order: spy, spy, REPLACE, spy, REPLACE,
/// spy, REPLACE, spy (activation order). The walk is top-of-stack first, so it
/// visits #8(spy)..#1, counting each spy until it hits the first replacement from
/// the top. From the top: #8 spy, #7 spy, #6 REPLACE -> claims. Only #8,#7,#6
/// count; #6's replacement fires. #5..#1 never reached.
#[tokio::test]
async fn torture_precedence_conflict_03_eight_on_one_key_first_replace_from_top_wins() {
    let output = baml_test!(
        r#"
        function f() -> string { "real" }

        function main() -> int {
            let m1 = baml.mock.new(f);                       // spy
            let m2 = baml.mock.new(f);                       // spy
            let m3 = baml.mock.new(f); m3.replace(() -> string { "three" });
            let m4 = baml.mock.new(f);                       // spy
            let m5 = baml.mock.new(f); m5.replace(() -> string { "five" });
            let m6 = baml.mock.new(f);                       // spy
            let m7 = baml.mock.new(f); m7.replace(() -> string { "seven" });
            let m8 = baml.mock.new(f);                       // spy

            let r = "";
            baml.mock.scope([m1, m2, m3, m4, m5, m6, m7, m8], () -> void {
                r = f();   // top-down: m8 spy, m7 REPLACE -> "seven"
            });
            // m8 counted (1), m7 counted+claims (1), nothing below reached.
            r.length() * 1000000
              + m8.call_count * 100000 + m7.call_count * 10000 + m6.call_count * 1000
              + m5.call_count * 100 + m4.call_count * 10 + m3.call_count
            // "seven" len 5 -> 5*1e6 + m8=1*1e5 + m7=1*1e4 + m6=0 + m5=0 + m4=0 + m3=0
            // = 5000000 + 100000 + 10000 = 5110000
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(5110000)));
}

/// The SAME Mock object scoped twice (nested) on one key. It appears twice in the
/// stack; the `seen` dedup must count it only ONCE per call even though it is
/// present at two stack positions. call_count after three nested-extent calls == 3
/// (not 6).
#[tokio::test]
async fn torture_precedence_conflict_04_same_mock_nested_seen_dedup_no_double_count() {
    let output = baml_test!(
        r#"
        function f() -> int { 0 }

        function main() -> int {
            let m = baml.mock.new(f);
            m.replace(() -> int { 0 });
            baml.mock.scope(m, () -> void {            // pushed once
                let _ = f();                           // count 1 (one entry, deduped)
                baml.mock.scope(m, () -> void {        // pushed again (now twice in stack)
                    let _ = f();                       // count 2 (seen-dedup: counted once)
                    let _ = f();                       // count 3
                });
            });
            m.call_count                               // 3, not 6
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(3)));
}

/// The SAME Mock object passed BOTH as a single-scope arg AND inside an array
/// scope simultaneously. It is pushed three times across the two scope calls; the
/// `already_active` guard must reset the counter only on the very first activation,
/// and `seen` must keep each call counting once. Verify the count is the number of
/// calls (4), not multiplied by stack depth.
#[tokio::test]
async fn torture_precedence_conflict_05_same_mock_in_array_and_single_scope() {
    let output = baml_test!(
        r#"
        function f() -> int { 1 }
        function g() -> int { 2 }

        function main() -> int {
            let m = baml.mock.new(f);
            m.replace(() -> int { 10 });
            let other = baml.mock.new(g);
            other.replace(() -> int { 20 });

            let acc = 0;
            baml.mock.scope(m, () -> void {                 // m active (1st: reset to 0)
                acc = acc + f();                            // 10, count 1
                baml.mock.scope([other, m], () -> void {    // m active again (no reset)
                    acc = acc + f();                        // 10, count 2 (deduped)
                    acc = acc + f();                        // 10, count 3
                });
                acc = acc + f();                            // 10, count 4
            });
            acc * 100 + m.call_count                        // 40*100 + 4 = 4004
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(4004)));
}

/// Re-replace a mock mid-scope: `m.replace(X)` then inside the scope `m.replace(Y)`
/// before the first call. `.replace` takes effect immediately, so the call hits Y.
/// Then a third replace swaps to Z mid-scope; the next call hits Z.
#[tokio::test]
async fn torture_precedence_conflict_06_re_replace_mid_scope_latest_wins() {
    let output = baml_test!(
        r#"
        function f() -> int { 0 }

        function main() -> int {
            let m = baml.mock.new(f);
            m.replace(() -> int { 1 });               // X
            let log = 0;
            baml.mock.scope(m, () -> void {
                m.replace(() -> int { 2 });           // Y, takes effect immediately
                log = log * 10 + f();                 // 2
                m.replace(() -> int { 3 });           // Z
                log = log * 10 + f();                 // 3
            });
            let after = f();                           // scope ended -> real 0
            log * 10 + after                           // 2,3 -> 23, then *10 + 0 = 230
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(230)));
}

/// A replacement that is ITSELF a mocked function. `outer` mocks `f`; its
/// replacement body calls `g`. `g` is ALSO mocked. So a call to `f` redirects to
/// the replacement, which then dispatches `g` -> g's replacement. Two distinct
/// mocks fire on one logical call path, each counting once.
#[tokio::test]
async fn torture_precedence_conflict_07_replacement_calls_a_mocked_function() {
    let output = baml_test!(
        r#"
        function f() -> int { 0 }
        function g() -> int { 0 }

        function main() -> int {
            let mf = baml.mock.new(f);
            mf.replace(() -> int { g() + 1 });        // replacement body calls g
            let mg = baml.mock.new(g);
            mg.replace(() -> int { 100 });            // g is mocked too

            let r = 0;
            baml.mock.scope(mf, () -> void {
                baml.mock.scope(mg, () -> void {
                    r = f();                          // f->repl: g()=100 -> 101
                });
            });
            // f counted 1, g counted 1.
            r * 100 + mf.call_count * 10 + mg.call_count   // 101*100 + 10 + 1 = 10111
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(10111)));
}

/// A deep transparent chain: spy over spy over a replacement over the real fn, with
/// the replacement also delegating one step down by calling the target by name
/// (recursion guard steps to the REAL fn since the only replacement is suppressed).
/// Two spies counted, the replacement counted + claims, by-name call inside it
/// reaches the real fn. Tests spy transparency + suppression interplay.
#[tokio::test]
async fn torture_precedence_conflict_08_spy_over_spy_over_replace_delegating_to_real() {
    let output = baml_test!(
        r#"
        function f() -> string { "real" }

        function main() -> int {
            let rep = baml.mock.new(f);
            rep.replace(() -> string { "[" + f() + "]" });   // delegates down by name
            let s1 = baml.mock.new(f);                        // spy
            let s2 = baml.mock.new(f);                        // spy (topmost)

            let r = "";
            baml.mock.scope(rep, () -> void {
                baml.mock.scope(s1, () -> void {
                    baml.mock.scope(s2, () -> void {
                        r = f();   // s2 spy, s1 spy, rep claims -> "[" + f() + "]"
                                   // inner f(): rep suppressed, s2/s1 spies -> real "real"
                                   // -> "[real]"
                    });
                });
            });
            // "[real]" len 6. s2 counted at outer call (1). s1 counted (1).
            // rep counted (1). Inner f(): the recursion guard suppresses rep, but
            // s1/s2 spies are NOT suppressed and ARE reached again -> they count a
            // 2nd time, then fall through to the real fn.
            r.length() * 1000 + s2.call_count * 100 + s1.call_count * 10 + rep.call_count
            // 6*1000 + s2=2*100 + s1=2*10 + rep=1 = 6000 + 200 + 20 + 1 = 6221
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(6221)));
}

/// Generic axis conflict: one call matches BOTH a Generic specialization key AND
/// the bare Free key. The walk probes Generic first; if it has a replacement it
/// wins and the bare mock is not reached. Here the specialization is a SPY, so it
/// counts and the walk continues to the bare replacement. Both count.
#[tokio::test]
async fn torture_precedence_conflict_09_generic_spy_falls_through_to_bare_replace() {
    let output = baml_test!(
        r#"
        function id<T>(x: T) -> T { x }

        function main() -> int {
            let spec = baml.mock.new(id<int>);        // Generic key, pure spy
            let bare = baml.mock.new(id);             // Free key, replacement
            bare.replace(<T>(x: T) -> T { x });

            let r = 0;
            baml.mock.scope(bare, () -> void {
                baml.mock.scope(spec, () -> void {
                    r = id<int>(7);   // Generic spy counts, falls to bare repl -> 7
                });
            });
            // spec counted 1 (spy transparent), bare counted 1 (claims).
            r * 100 + spec.call_count * 10 + bare.call_count   // 7*100 + 10 + 1 = 711
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(711)));
}

/// Generic specialization replacement SHADOWS the bare replacement for `id<int>`
/// but NOT for `id<string>`. One scope holds both; the int call hits spec, the
/// string call falls through Generic(miss) to the bare replacement. Verifies the
/// Generic key only matches the exact type-arg set.
#[tokio::test]
async fn torture_precedence_conflict_10_generic_specialization_shadows_only_its_instantiation() {
    let output = baml_test!(
        r#"
        function id<T>(x: T) -> T { x }

        function main() -> int {
            let spec = baml.mock.new(id<int>);
            spec.replace((x: int) -> int { x + 1000 });
            let bare = baml.mock.new(id);
            bare.replace(<T>(x: T) -> T { x });

            let a = 0;
            let blen = 0;
            baml.mock.scope([bare, spec], () -> void {
                a = id<int>(5);                       // Generic spec wins -> 1005
                blen = id<string>("ab").length();     // no <string> spec -> bare -> "ab" len 2
            });
            // spec counted only for the int call (1), bare counted only for string (1).
            a * 1000 + blen * 100 + spec.call_count * 10 + bare.call_count
            // 1005*1000 + 2*100 + 1*10 + 1 = 1005000 + 200 + 10 + 1 = 1005211
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(1005211)));
}

/// Adversarial nesting where the INNERMOST mock is a pure spy and every layer below
/// it is a replacement. The walk counts the spy and delegates to the topmost
/// replacement (the next one down). The lower replacements are never reached
/// because the first replacement from the top claims. Verifies "innermost-wins"
/// resolves to the innermost *replacement*, not the innermost mock.
#[tokio::test]
async fn torture_precedence_conflict_11_innermost_spy_delegates_to_next_replacement() {
    let output = baml_test!(
        r#"
        function f() -> string { "real" }

        function main() -> int {
            let a = baml.mock.new(f); a.replace(() -> string { "A" });
            let b = baml.mock.new(f); b.replace(() -> string { "B" });
            let c = baml.mock.new(f); c.replace(() -> string { "C" });
            let spy = baml.mock.new(f);               // innermost, pure spy

            let r = "";
            baml.mock.scope(a, () -> void {
                baml.mock.scope(b, () -> void {
                    baml.mock.scope(c, () -> void {
                        baml.mock.scope(spy, () -> void {
                            r = f();   // spy counts, delegates to c -> "C"
                        });
                    });
                });
            });
            // spy 1, c 1 (claims). b and a never reached -> 0.
            r.length() * 10000 + spy.call_count * 1000 + c.call_count * 100
              + b.call_count * 10 + a.call_count
            // "C" len 1 -> 1*10000 + spy=1*1000 + c=1*100 + b=0 + a=0 = 11100
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(11100)));
}

/// Pathological: a replacement on `f` whose body re-invokes `f` by name in a loop,
/// with a SPY also active above the replacement. The recursion guard suppresses the
/// claiming replacement on re-entry, so the by-name call steps down to the real fn
/// — it must TERMINATE. The spy above is not suppressed, so it counts on every
/// re-entry. We call the replacement once but it re-enters f three times.
#[tokio::test]
async fn torture_precedence_conflict_12_replacement_reenters_with_spy_above_terminates() {
    let output = baml_test!(
        r#"
        function f() -> int { 1 }

        function main() -> int {
            let rep = baml.mock.new(f);
            // Body calls f() three times by name; each re-entry steps down (guard).
            rep.replace(() -> int { f() + f() + f() });   // 1 + 1 + 1 = 3
            let spy = baml.mock.new(f);                    // spy above the replacement

            let r = 0;
            baml.mock.scope(rep, () -> void {
                baml.mock.scope(spy, () -> void {
                    r = f();   // spy counts, rep claims; inner 3x f() each hit real (1)
                });
            });
            // Outer call: spy 1, rep 1. Each of the 3 inner f(): rep suppressed,
            // spy still reached -> spy +3 more (total 4), rep stays 1, real runs.
            r * 100 + spy.call_count * 10 + rep.call_count   // 3*100 + 4*10 + 1 = 341
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(341)));
}

/// Mutual mock recursion that MUST terminate: `f`'s replacement calls `g`, `g`'s
/// replacement calls `f`. Each by-name call is in a fresh frame; the recursion
/// guard suppresses only the same mock currently running, so f->g->f steps to the
/// real f. Could infinite-loop if the guard fails to step down.
#[tokio::test]
async fn torture_precedence_conflict_13_mutual_replacement_recursion_terminates() {
    let output = baml_test!(
        r#"
        function f() -> int { 1 }
        function g() -> int { 2 }

        function main() -> int {
            let mf = baml.mock.new(f);
            mf.replace(() -> int { g() * 10 });       // f-repl calls g
            let mg = baml.mock.new(g);
            mg.replace(() -> int { f() });            // g-repl calls f (steps to real f=1)

            let r = 0;
            baml.mock.scope(mf, () -> void {
                baml.mock.scope(mg, () -> void {
                    r = f();   // f-repl -> g-repl -> f (mf suppressed) -> real 1 -> *10 = 10
                });
            });
            r * 10 + mf.call_count * 100 + mg.call_count
            // r=10 -> 100 + mf? : f called twice (outer claim + inner by-name).
            // Outer f(): mf claims (count 1). Inner f() from g-repl: mf suppressed
            // -> not counted, real runs. So mf=1, mg=1 (g called once).
            // 10*10 + 1*100 + 1 = 100 + 100 + 1 = 201
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(201)));
}

/// Re-replace a mock to a DIFFERENT replacement, then nest a second scope of the
/// SAME mock — the inner scope does not push a fresh layer that resets behavior;
/// the (single, latest) replacement applies at every nesting depth. Combined with
/// a re-replace at the deepest level that the outer level then observes (the mock
/// object is shared, so the swap is global to the mock).
#[tokio::test]
async fn torture_precedence_conflict_14_re_replace_visible_after_nested_same_scope() {
    let output = baml_test!(
        r#"
        function f() -> int { 0 }

        function main() -> int {
            let m = baml.mock.new(f);
            m.replace(() -> int { 1 });
            let log = 0;
            baml.mock.scope(m, () -> void {
                log = log * 10 + f();                 // 1
                baml.mock.scope(m, () -> void {       // same mock nested
                    log = log * 10 + f();             // still 1
                    m.replace(() -> int { 9 });       // swap inside the inner scope
                    log = log * 10 + f();             // 9
                });
                log = log * 10 + f();                 // swap persists (shared object) -> 9
            });
            log                                       // 1,1,9,9 -> 1199
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(1199)));
}

/// Class mock and interface mock both active, NO instance mock. The class (Free)
/// key is probed before the interface (Free, interface-derived) key, so the class
/// replacement wins for a Dog call. A Cat call (same interface, different class)
/// has no class mock, so it falls through to the interface mock. Verifies the
/// Free(class) vs Free(interface) ordering and per-class attribution.
#[tokio::test]
async fn torture_precedence_conflict_15_class_beats_interface_but_other_impl_falls_through() {
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

        function main() -> int {
            let cm = baml.mock.new(Dog.speak);          // class (Dog only)
            cm.replace((self: Dog) -> string { "DOG" });
            let am = baml.mock.new(Animal.speak);       // interface (all impls)
            am.replace((self: Animal) -> string { "ANY" });

            let d = Dog {};
            let c = Cat {};
            let rd = "";
            let rc = "";
            baml.mock.scope(am, () -> void {
                baml.mock.scope(cm, () -> void {
                    rd = d.speak();   // class wins -> "DOG"
                    rc = c.speak();   // no class mock for Cat -> interface -> "ANY"
                });
            });
            // d.speak: cm claims (count 1), am not reached. c.speak: cm key is
            // Dog.speak, doesn't match Cat's class key; interface am claims (count 1).
            rd.length() * 1000 + rc.length() * 100 + cm.call_count * 10 + am.call_count
            // "DOG" len 3, "ANY" len 3 -> 3*1000 + 3*100 + 1*10 + 1 = 3311
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(3311)));
}

/// Six mocks on one key, all PURE SPIES (no replacement anywhere). The walk reaches
/// every spy (each counts) and, finding no replacement, runs the original. Verifies
/// the all-spy fall-through counts every layer and returns the real value.
#[tokio::test]
async fn torture_precedence_conflict_16_six_stacked_spies_all_count_run_original() {
    let output = baml_test!(
        r#"
        function f() -> int { 7 }

        function main() -> int {
            let a = baml.mock.new(f);
            let b = baml.mock.new(f);
            let c = baml.mock.new(f);
            let d = baml.mock.new(f);
            let e = baml.mock.new(f);
            let g = baml.mock.new(f);

            let r = 0;
            baml.mock.scope([a, b, c, d, e, g], () -> void {
                r = f();   // all six spies counted; original runs -> 7
            });
            // each spy count 1; sum 6. r=7.
            r * 100
              + (a.call_count + b.call_count + c.call_count
                 + d.call_count + e.call_count + g.call_count)
            // 7*100 + 6 = 706
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(706)));
}

/// Instance mock on receiver `a` AND a class mock; a SECOND instance `b` shares the
/// class mock only. One call on `a` (instance wins) and one on `b` (instance key
/// for `b` absent -> class mock). The instance mock's strip_self path and the class
/// mock's self-keeping path are both exercised in one scope. Verifies receiver
/// keying does not cross-fire and each call_count attributes correctly.
#[tokio::test]
async fn torture_precedence_conflict_17_instance_and_class_two_receivers_no_crossfire() {
    let output = baml_test!(
        r#"
        class Counter {
          count int
          function bump(self) -> int { self.count + 1 }
        }

        function main() -> int {
            let a = Counter { count: 0 };
            let b = Counter { count: 50 };
            let im = baml.mock.new(a.bump);             // instance (a only)
            im.replace(() -> int { 1000 });
            let cm = baml.mock.new(Counter.bump);       // class (all)
            cm.replace((self: Counter) -> int { self.count + 1 });   // class echoes real

            let ra = 0;
            let rb = 0;
            baml.mock.scope(cm, () -> void {
                baml.mock.scope(im, () -> void {
                    ra = a.bump();   // instance wins -> 1000 (cm not reached)
                    rb = b.bump();   // no instance for b -> class -> 50+1 = 51
                });
            });
            // im counts only a's call (1). cm counts only b's call (1).
            ra + rb * 10000 + im.call_count * 100 + cm.call_count
            // 1000 + 51*10000 + 1*100 + 1 = 1000 + 510000 + 100 + 1 = 511101
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(511101)));
}

/// Spawn snapshot under a deep precedence stack: inside a scope of [class spy,
/// instance replacement], a spawn calls the mocked instance method. The snapshot
/// must carry BOTH mocks and the Instance-key receiver pointer across the VM
/// boundary, so the spawn resolves instance (replacement, count++) with the class
/// spy transparent above it. Verifies precedence + spy transparency survive the
/// parent->child mock_table clone.
#[tokio::test]
async fn torture_precedence_conflict_18_spawn_preserves_precedence_spy_over_instance() {
    let output = baml_test!(
        r#"
        class Counter {
          count int
          function bump(self) -> int { self.count + 1 }
        }

        function main() -> int {
            let a = Counter { count: 0 };
            let cm = baml.mock.new(Counter.bump);       // class spy (no replace)
            let im = baml.mock.new(a.bump);             // instance replacement
            im.replace(() -> int { 99 });

            let r = 0;
            baml.mock.scope(cm, () -> void {
                baml.mock.scope(im, () -> void {
                    let fut = spawn { a.bump() };   // snapshot carries cm + im
                    r = await fut;                  // instance claims -> 99 (class spy transparent? no — instance replaces first)
                });
            });
            // In the spawn, candidates: Instance(a) [im, replacement] -> claims
            // BEFORE class spy is even probed (Instance key precedes Free key).
            // So im counts 1, cm counts 0.
            r * 100 + im.call_count * 10 + cm.call_count   // 99*100 + 1*10 + 0 = 9910
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(9910)));
}
