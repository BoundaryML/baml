use baml_tests::baml_test;
use bex_engine::BexExternalValue;

/// A class implements TWO interfaces that BOTH declare `encode`; mocking ONE
/// interface must fire only for calls routed through THAT interface
/// (`h.as<Serializer>.encode()` -> derived key `user.Serializer.encode`), and
/// must NOT bleed into the sibling interface's call
/// (`h.as<BinarySerializer>.encode()` -> `user.BinarySerializer.encode`). The
/// two impl methods have distinct FQ names (`user.Hybrid.Serializer.encode` vs
/// `user.Hybrid.BinarySerializer.encode`), so the interface_method_key derivation
/// must keep them apart.
#[tokio::test]
async fn torture_interface_01_two_ifaces_same_method_mock_one_only() {
    let output = baml_test!(
        r#"
        interface Serializer { function encode(self) -> string }
        interface BinarySerializer { function encode(self) -> string }
        class Hybrid {
          implements Serializer { function encode(self) -> string { "json" } }
          implements BinarySerializer { function encode(self) -> string { "bin" } }
        }

        function main() -> string {
            let m = baml.mock.new(Serializer.encode);
            m.replace((self: Serializer) -> string { "MOCKED" });
            let h = Hybrid {};
            let viaS = "";
            let viaB = "";
            baml.mock.scope(m, () -> void {
                viaS = h.as<Serializer>.encode();        // mocked -> "MOCKED"
                viaB = h.as<BinarySerializer>.encode();  // untouched -> "bin"
            });
            viaS + "|" + viaB                            // "MOCKED|bin"
        }
        "#
    );
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("MOCKED|bin".into()))
    );
}

/// Both sibling interfaces (same method name, one class) mocked independently in
/// the same scope. Each derived key must route to its own replacement with no
/// cross-fire; if the key derivation collapsed both impl methods to one key, one
/// replacement would shadow the other. Observability via string concat + each
/// mock's call_count (must be exactly 1 each — no double-counting from the
/// sibling key).
#[tokio::test]
async fn torture_interface_02_both_sibling_ifaces_mocked_no_crossfire() {
    let output = baml_test!(
        r#"
        interface Serializer { function encode(self) -> string }
        interface BinarySerializer { function encode(self) -> string }
        class Hybrid {
          implements Serializer { function encode(self) -> string { "json" } }
          implements BinarySerializer { function encode(self) -> string { "bin" } }
        }

        function main() -> int {
            let ms = baml.mock.new(Serializer.encode);
            ms.replace((self: Serializer) -> string { "S" });
            let mb = baml.mock.new(BinarySerializer.encode);
            mb.replace((self: BinarySerializer) -> string { "B" });
            let h = Hybrid {};
            let r = "";
            baml.mock.scope([ms, mb], () -> void {
                r = h.as<Serializer>.encode() + h.as<BinarySerializer>.encode();
            });
            // r == "SB" (len 2). Each mock counts exactly once (no cross-fire).
            r.length() * 100 + ms.call_count * 10 + mb.call_count   // 200 + 10 + 1 = 211
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(211)));
}

/// Three implementors of one interface; mock the interface, call all three. The
/// single interface mock must fire for every implementor and the shared
/// call_count must total 3. Stresses interface_method_key over multiple distinct
/// concrete callee names (`user.A.Shape.area`, `user.B.Shape.area`,
/// `user.C.Shape.area`) all mapping back to `user.Shape.area`.
#[tokio::test]
async fn torture_interface_03_three_implementors_one_iface_mock() {
    let output = baml_test!(
        r#"
        interface Shape { function area(self) -> int }
        class A { implements Shape { function area(self) -> int { 1 } } }
        class B { implements Shape { function area(self) -> int { 2 } } }
        class C { implements Shape { function area(self) -> int { 3 } } }

        function main() -> int {
            let m = baml.mock.new(Shape.area);
            m.replace((self: Shape) -> int { 100 });
            let a: Shape = A {};
            let b: Shape = B {};
            let c: Shape = C {};
            let sum = 0;
            baml.mock.scope(m, () -> void {
                sum = a.area() + b.area() + c.area();   // 100*3 = 300
            });
            sum + m.call_count                           // 300 + 3 = 303
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(303)));
}

/// interface + class(Free) + instance mocks ALL on one call: precedence must be
/// instance > class > interface (narrowest call set wins). Only the instance
/// replacement should run; class and interface mocks observe nothing here (the
/// walk stops at the first replacement, most-specific key first).
#[tokio::test]
async fn torture_interface_04_iface_class_instance_precedence_all_active() {
    let output = baml_test!(
        r#"
        interface Animal { function speak(self) -> int }
        class Dog { implements Animal { function speak(self) -> int { 1 } } }

        function main() -> int {
            let d = Dog {};
            let mi = baml.mock.new(Animal.speak);
            mi.replace((self: Animal) -> int { 5 });
            let mc = baml.mock.new(Dog.speak);
            mc.replace((self: Dog) -> int { 7 });
            let mn = baml.mock.new(d.speak);
            mn.replace(() -> int { 9 });
            let r = 0;
            baml.mock.scope([mi, mc, mn], () -> void {
                r = d.speak();          // instance wins -> 9
            });
            // instance count 1; class + interface should be 0 (walk stopped).
            r * 100 + mn.call_count * 10 + mc.call_count + mi.call_count   // 910
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(910)));
}

/// Same method name across an interface and an UNRELATED class (no `implements`).
/// Mocking the interface (`Animal.speak` -> derived `user.Animal.speak`) must NOT
/// intercept the unrelated `Robot.speak` (concrete callee `user.Robot.speak`,
/// no interface segment, so interface_method_key returns None for it). And the
/// `Robot.speak` Free mock must not touch the interface implementor.
#[tokio::test]
async fn torture_interface_05_unrelated_class_same_method_no_bleed() {
    let output = baml_test!(
        r#"
        interface Animal { function speak(self) -> int }
        class Dog { implements Animal { function speak(self) -> int { 1 } } }
        class Robot { function speak(self) -> int { 2 } }   // no implements

        function main() -> int {
            let mi = baml.mock.new(Animal.speak);
            mi.replace((self: Animal) -> int { 50 });
            let mr = baml.mock.new(Robot.speak);
            mr.replace((self: Robot) -> int { 70 });
            let d: Animal = Dog {};
            let rob = Robot {};
            let log = 0;
            baml.mock.scope(mi, () -> void {
                log = log * 10 + d.speak();     // iface mock -> 50  (5)
                log = log * 10 + rob.speak();   // unrelated, mi inactive on it -> 2
            });
            baml.mock.scope(mr, () -> void {
                log = log * 10 + rob.speak();   // robot mock -> 70
                log = log * 10 + d.speak();     // dog unaffected -> real 1
            });
            log   // 50 -> 502 -> 50270 -> 502701  (truncated by int width? trace below)
        }
        "#
    );
    // log: 0 ->50 ->502 ->5027 (502*10+70=5090? recompute): start 0
    //  *10+50 = 50
    //  *10+2  = 502
    //  *10+70 = 5090
    //  *10+1  = 50901
    assert_eq!(output.result, Ok(BexExternalValue::Int(50901)));
}

/// A DEFAULT interface method (body on the interface, not overridden) is mocked
/// via the interface. The default method lowers to a plain Function constant
/// (`user.Describable.describe`), and a non-overriding implementor dispatches to
/// that same function, so the Free-key mock must fire.
#[tokio::test]
async fn torture_interface_06_default_method_mock_fires() {
    let output = baml_test!(
        r#"
        interface Describable {
          function describe(self) -> string { "default" }
        }
        class Thing { implements Describable {} }

        function main() -> string {
            let m = baml.mock.new(Describable.describe);
            m.replace((self: Describable) -> string { "mocked" });
            let t: Describable = Thing {};
            let r = "";
            baml.mock.scope(m, () -> void {
                r = t.describe();        // default method, mocked -> "mocked"
            });
            r + ":" + t.describe()       // outside -> "default"
        }
        "#
    );
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("mocked:default".into()))
    );
}

/// A default method that one class OVERRIDES and another inherits. Mocking the
/// interface default-method value keys on the DEFAULT's FQ name. The overriding
/// class dispatches to its own `user.Cat.Animal.describe` (interface-derived key
/// `user.Animal.describe`); the inheriting class dispatches to the default
/// function itself. Whether ONE interface mock covers BOTH depends on whether the
/// default FQ name equals the derived key — a divergence probe.
#[tokio::test]
async fn torture_interface_07_default_vs_override_one_mock_coverage() {
    let output = baml_test!(
        r#"
        interface Animal {
          function describe(self) -> string { "DFLT" }
        }
        class Cat {
          implements Animal { function describe(self) -> string { "cat" } }
        }
        class Thing { implements Animal {} }   // inherits default

        function main() -> string {
            let m = baml.mock.new(Animal.describe);
            m.replace((self: Animal) -> string { "M" });
            let c: Animal = Cat {};
            let t: Animal = Thing {};
            let r = "";
            baml.mock.scope(m, () -> void {
                r = c.describe() + t.describe();   // both implementors mocked -> "MM"
            });
            r
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::String("MM".into())));
}

/// A generic interface mock on a SINGLE-specialization class: `Converter<int>`
/// is the only Converter `M` implements. Mock the interface method, call via the
/// projection. The interface method value lowers to an InterfaceMethodRef keyed
/// `Free("user.Converter.convert")` (the type arg is NOT carried — BEP Open
/// Question #1: bounds/specialization on the lambda are dropped at lowering), and
/// the impl method `user.M.Converter.convert` derives the same key, so a
/// non-bounded replacement fires for every Converter call on M.
#[tokio::test]
async fn torture_interface_08_generic_interface_method_single_spec() {
    let output = baml_test!(
        r#"
        interface Converter<T> { function convert(self) -> T }
        class M {
          implements Converter<int> { function convert(self) -> int { 1 } }
        }
        function read_int<T extends Converter<int>>(m: T) -> int {
            m.as<Converter<int>>.convert()
        }

        function main() -> int {
            let mk = baml.mock.new(Converter<int>.convert);
            mk.replace((self: Converter<int>) -> int { 99 });
            let m = M {};
            let r = 0;
            baml.mock.scope(mk, () -> void {
                r = read_int<M>(m);     // mocked Converter<int>.convert -> 99
            });
            r + mk.call_count           // 99 + 1 = 100
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(100)));
}

/// Interface mock + recursion: the replacement calls the method BY NAME on the
/// receiver (re-entering the same dispatch). The recursion guard (mock_suppress)
/// must step one down to the real implementor method, not loop forever. Per the
/// guard spec it terminates: by-name re-entry is suppressed and the real fn runs.
#[tokio::test]
async fn torture_interface_09_iface_replacement_reenters_by_name() {
    let output = baml_test!(
        r#"
        interface Greeter { function greet(self) -> string }
        class Person {
          implements Greeter { function greet(self) -> string { "real" } }
        }

        function main() -> string {
            let m = baml.mock.new(Greeter.greet);
            m.replace((self: Greeter) -> string { "[" + self.greet() + "]" });
            let p: Greeter = Person {};
            let r = "";
            baml.mock.scope(m, () -> void {
                r = p.greet();   // re-enter steps down to real -> "[real]"
            });
            r
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::String("[real]".into())));
}

/// Interface mock stacked OVER a class mock on the same call, both with
/// replacements. The interface key is LEAST specific, the class (Free) key more
/// specific. Per precedence the class replacement wins even though the interface
/// mock was activated innermost; nesting order does not override specificity.
#[tokio::test]
async fn torture_interface_10_class_beats_iface_regardless_of_nesting() {
    let output = baml_test!(
        r#"
        interface Animal { function speak(self) -> int }
        class Dog { implements Animal { function speak(self) -> int { 1 } } }

        function main() -> int {
            let mc = baml.mock.new(Dog.speak);
            mc.replace((self: Dog) -> int { 7 });
            let mi = baml.mock.new(Animal.speak);
            mi.replace((self: Animal) -> int { 5 });
            let d: Animal = Dog {};
            let r = 0;
            // interface activated innermost, but class key is more specific.
            baml.mock.scope(mc, () -> void {
                baml.mock.scope(mi, () -> void {
                    r = d.speak();    // class wins -> 7
                });
            });
            r * 10 + mi.call_count   // class wins -> 70; iface observed nothing -> 0
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(70)));
}

/// Interface mock visible across a `spawn`: an interface-keyed mock must survive
/// the parent->child VM mock_table clone and fire for an implementor call made
/// inside the spawn, with the shared call_count aggregating it.
#[tokio::test]
async fn torture_interface_11_iface_mock_seen_in_spawn() {
    let output = baml_test!(
        r#"
        interface Animal { function speak(self) -> int }
        class Dog { implements Animal { function speak(self) -> int { 1 } } }

        function main() -> int {
            let m = baml.mock.new(Animal.speak);
            m.replace((self: Animal) -> int { 42 });
            let d: Animal = Dog {};
            let r = 0;
            baml.mock.scope(m, () -> void {
                let f = spawn { d.speak() };   // iface mock visible in spawn
                r = await f;                    // 42
            });
            r * 10 + m.call_count               // 42*10 + 1 = 421
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(421)));
}

/// A pure-spy interface mock stacked over a class-mock REPLACEMENT. The spy is on
/// the least-specific (interface) key; the replacement on the more-specific class
/// key. The walk visits the class key first (replacement claims), so the
/// interface spy below it is NEVER reached — its call_count must stay 0. Probes
/// that specificity ordering, not stack order, controls the walk.
#[tokio::test]
async fn torture_interface_12_iface_spy_under_class_replacement_unreached() {
    let output = baml_test!(
        r#"
        interface Animal { function speak(self) -> int }
        class Dog { implements Animal { function speak(self) -> int { 1 } } }

        function main() -> int {
            let spy = baml.mock.new(Animal.speak);   // pure spy, interface key
            let mc = baml.mock.new(Dog.speak);
            mc.replace((self: Dog) -> int { 7 });
            let d: Animal = Dog {};
            let r = 0;
            baml.mock.scope([spy, mc], () -> void {
                r = d.speak();        // class replacement claims first -> 7
            });
            // class replacement ran (count 1); interface spy never reached (0).
            r * 100 + mc.call_count * 10 + spy.call_count   // 700 + 10 + 0 = 710
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(710)));
}

/// Interface mock called on a CONCRETE-typed (not interface-typed) receiver.
/// The in-body impl method's runtime FQ name carries the interface segment
/// (`user.Dog.Animal.speak`) regardless of the static receiver type, so the
/// interface_method_key derivation fires the interface mock even for a concrete
/// `Dog` value. Per BEP target #5 (every implementor) the concrete value IS an
/// implementor, so the mock fires.
#[tokio::test]
async fn torture_interface_13_concrete_receiver_iface_mock_fires() {
    let output = baml_test!(
        r#"
        interface Animal { function speak(self) -> int }
        class Dog { implements Animal { function speak(self) -> int { 1 } } }

        function main() -> int {
            let m = baml.mock.new(Animal.speak);
            m.replace((self: Animal) -> int { 99 });
            let d = Dog {};         // concrete-typed, NOT `: Animal`
            let r = 0;
            baml.mock.scope(m, () -> void {
                r = d.speak();      // in-body impl name has iface segment -> 99
            });
            r + m.call_count        // 99 + 1 = 100
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(100)));
}

/// Diamond defaults: `Left` and `Right` both `requires Base`, both declare
/// `foo`. A class implements all three. Mocking ONLY `Left.foo` must intercept
/// `d.as<Left>.foo()` but leave `d.as<Right>.foo()` and `d.as<Base>.foo()`
/// intact. Stresses interface_method_key disambiguation across a requires-diamond.
#[tokio::test]
async fn torture_interface_14_diamond_mock_left_only() {
    let output = baml_test!(
        r#"
        interface Base { function foo(self) -> string { "Base" } }
        interface Left requires Base { function foo(self) -> string { "Left" } }
        interface Right requires Base { function foo(self) -> string { "Right" } }
        class D {
          implements Base {}
          implements Left {}
          implements Right {}
        }

        function main() -> string {
            let m = baml.mock.new(Left.foo);
            m.replace((self: Left) -> string { "M" });
            let d = D {};
            let r = "";
            baml.mock.scope(m, () -> void {
                r = d.as<Left>.foo() + d.as<Right>.foo() + d.as<Base>.foo();
            });
            r   // only Left mocked -> "MRightBase"
        }
        "#
    );
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("MRightBase".into()))
    );
}

/// Interface mock whose replacement THROWS. The throw must propagate as the
/// call's error and unwind the scope (deactivating the mock), and a faulting
/// mocked call still counts. After the caught throw, the implementor call hits
/// the real method again.
#[tokio::test]
async fn torture_interface_15_iface_replacement_throws_propagates_and_pops() {
    let output = baml_test!(
        r#"
        interface Animal { function speak(self) -> int throws string }
        class Dog {
          implements Animal {
            function speak(self) -> int throws string { 1 }
          }
        }

        function main() -> int {
            let m = baml.mock.new(Animal.speak);
            m.replace((self: Animal) -> int throws string { throw "boom" });
            let d: Animal = Dog {};
            let caught = 0;
            baml.mock.scope(m, () -> void {
                let _ = d.speak() catch (e) { _ => { caught = 1; 0 } };
            });
            let after = d.speak() catch (e) { _ => { 0 } };   // scope popped -> real 1
            caught * 1000 + after * 10 + m.call_count          // 1000 + 10 + 1 = 1011
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(1011)));
}

/// Re-entrant interface dispatch through a DIFFERENT implementor inside the
/// replacement: the replacement, given a `Dog`, calls `speak()` on a freshly
/// constructed `Cat`. The recursion guard suppresses only the in-flight mock for
/// the SAME re-entry; a call on a different implementor under the same interface
/// key must ALSO be suppressed (by-name re-entry steps down) so the Cat's real
/// method runs, not the replacement again — no infinite loop.
#[tokio::test]
async fn torture_interface_16_reentrant_through_other_implementor() {
    let output = baml_test!(
        r#"
        interface Animal { function speak(self) -> string }
        class Dog { implements Animal { function speak(self) -> string { "dog" } } }
        class Cat { implements Animal { function speak(self) -> string { "cat" } } }

        function main() -> string {
            let m = baml.mock.new(Animal.speak);
            m.replace((self: Animal) -> string {
                let other: Animal = Cat {};
                "<" + other.speak() + ">"   // re-enter via Cat: guard steps down -> "cat"
            });
            let d: Animal = Dog {};
            let r = "";
            baml.mock.scope(m, () -> void {
                r = d.speak();   // replacement runs, inner Cat.speak steps down -> "<cat>"
            });
            r
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::String("<cat>".into())));
}

/// Bare class mock (Free key) vs interface mock when the SAME class is called
/// both via interface projection and via concrete dispatch in one scope, both
/// keys active. The class-key replacement is more specific than the interface
/// key; for any call to this class's impl method, the class mock claims it and
/// the interface mock observes nothing — even when invoked through `.as<I>`.
#[tokio::test]
async fn torture_interface_17_class_key_shadows_iface_through_projection() {
    let output = baml_test!(
        r#"
        interface Animal { function speak(self) -> int }
        class Dog { implements Animal { function speak(self) -> int { 1 } } }

        function main() -> int {
            let mc = baml.mock.new(Dog.speak);
            mc.replace((self: Dog) -> int { 7 });
            let mi = baml.mock.new(Animal.speak);
            mi.replace((self: Animal) -> int { 5 });
            let d = Dog {};
            let log = 0;
            baml.mock.scope([mi, mc], () -> void {
                log = log * 10 + d.as<Animal>.speak();   // class key wins -> 7
                log = log * 10 + d.speak();              // class key wins -> 7
            });
            // class observed both (2), interface observed nothing (0).
            log * 100 + mc.call_count * 10 + mi.call_count   // 7700 + 20 + 0 = 7720
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(7720)));
}

/// Mocking a multi-`Self` interface method is NOT interface-callable per BEP-044
/// (only single-`Self`/receiver methods are interface-mockable). Taking
/// `Comparable.cmp` (two `Self` params) as an interface method value should be
/// rejected at compile/lower time, yielding an Err.
#[tokio::test]
async fn torture_interface_18_multi_self_iface_method_not_mockable() {
    let output = baml_test!(
        r#"
        interface Comparable {
          function cmp(self, other: Self) -> int
        }
        class N {
          implements Comparable { function cmp(self, other: Self) -> int { 0 } }
        }

        function main() -> int {
            let m = baml.mock.new(Comparable.cmp);   // multi-Self: not interface-callable
            // mock.new rejects at runtime before replace runs; the replacement
            // just needs to type-check against the (Comparable, Comparable) -> int
            // value type the multi-Self method projects to through the interface.
            m.replace((self: Comparable, other: Comparable) -> int { 1 });
            0
        }
        "#
    );
    assert!(output.result.is_err());
}
