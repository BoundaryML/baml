// wf3 design-audit regressions, union-fuzz findings, and dispatch-guard
// regressions. See interfaces.baml for the LEFT IN RUST list.

// ─── wf3 design-audit regression suite ───────────────────────────────────────

// wf3_generic_requires_chain_default_delegation_runtime
interface W301Base<T> {
    function base_get(self) -> T throws never
}
interface W301Sub<T> requires W301Base<T> {
    function sub_get(self) -> T throws never {
        return self.base_get()
    }
}
class W301IntThing {
    value: int
    implements W301Base<int> {
        function base_get(self) -> int { return self.value }
    }
    implements W301Sub<int> {}
}
function w301_main() -> int {
    let t = W301IntThing { value: 77 }
    let s: W301Sub<int> = t
    return s.sub_get()
}
test "wf3_generic_requires_chain_default_delegation_runtime" {
    assert.equal(w301_main(), 77)
}

// wf3_blanket_on_blanket_chain_runtime
interface W302Named { name: string }
interface W302Printable { function display(self) -> string throws never }
interface W302Loud { function shout(self) -> string throws never }
class W302Person { name: string  implements W302Named {} }
implements<T extends W302Named> W302Printable for T {
    function display(self) -> string { return self.name }
}
implements<T extends W302Printable> W302Loud for T {
    function shout(self) -> string { return "loud" }
}
function w302_main() -> string {
    let p: W302Loud = W302Person { name: "Ada" }
    return p.shout()
}
test "wf3_blanket_on_blanket_chain_runtime" {
    assert.equal(w302_main(), "loud")
}

// wf3_default_field_access_does_not_crash
interface W304Named {
    name: string
    function describe(self) -> string throws never { return "x" }
}
class W304P {
    name: string
    implements W304Named {
        function describe(self) -> string {
            return "default.name=[" + default.name + "] self.name=[" + self.name + "] as=[" + self.as<W304Named>.name + "]"
        }
    }
}
function w304_main() -> string {
    let p = W304P { name: "Bob" }
    return p.describe()
}
test "wf3_default_field_access_does_not_crash" {
    assert.equal(w304_main(), "default.name=[Bob] self.name=[Bob] as=[Bob]")
}

// wf3_reflect_type_of_wrapped_generic_substitutes_param_runtime
interface W306Box<T> {
    function get(self) -> T throws never
}
function w306_names<U>() -> string {
    let naked = reflect.Type.of<U>().to_string()
    let wrapped = reflect.Type.of<W306Box<U>>().to_string()
    return "naked=" + naked + " wrapped=" + wrapped
}
function w306_main() -> string {
    return w306_names<int>()
}
test "wf3_reflect_type_of_wrapped_generic_substitutes_param_runtime" {
    assert.equal(w306_main(), "naked=int wrapped=interfaces.W306Box<int>")
}

// wf3_reflect_implemented_by_generic_arg_substitution_runtime
interface W307Box<T> {
    function get(self) -> T throws never
}
class W307IntBox {
    implements W307Box<int> {
        function get(self) -> int { return 1 }
    }
}
class W307StringBox {
    implements W307Box<string> {
        function get(self) -> string { return "hi" }
    }
}
function w307_box_impls_intbox<T>() -> bool {
    return reflect.Type.of<W307Box<T>>().implemented_by(reflect.Type.of<W307IntBox>())
}
function w307_box_impls_stringbox<T>() -> bool {
    return reflect.Type.of<W307Box<T>>().implemented_by(reflect.Type.of<W307StringBox>())
}
function w307_main() -> bool {
    return w307_box_impls_intbox<int>()
        && w307_box_impls_intbox<string>() == false
        && w307_box_impls_stringbox<string>()
        && w307_box_impls_stringbox<int>() == false
}
test "wf3_reflect_implemented_by_generic_arg_substitution_runtime" {
    assert.is_true(w307_main())
}

// wf3_pure_diamond_single_method_dispatches_runtime
interface W311Base { function id(self) -> string  throws never { return "base" } }
interface W311Left requires W311Base {}
interface W311Right requires W311Base {}
class W311D {
    implements W311Base {}
    implements W311Left {}
    implements W311Right {}
}
function w311_main() -> string {
    let d = W311D {}
    return d.id()
}
test "wf3_pure_diamond_single_method_dispatches_runtime" {
    assert.equal(w311_main(), "base")
}

// wf3_union_of_implementors_assignable_to_interface_runtime
interface W312Animal {
    function speak(self) -> string throws never
}
class W312Dog {
    implements W312Animal {
        function speak(self) -> string { return "Woof!" }
    }
}
class W312Cat {
    implements W312Animal {
        function speak(self) -> string { return "Meow." }
    }
}
function w312_as_animal(x: W312Dog | W312Cat) -> W312Animal {
    return x
}
function w312_main() -> string {
    let c: W312Cat = W312Cat {}
    let a = w312_as_animal(c)
    return a.speak()
}
test "wf3_union_of_implementors_assignable_to_interface_runtime" {
    assert.equal(w312_main(), "Meow.")
}

// wf3_out_of_body_primitive_impl_visible_to_reflection_runtime
// (Primitive method name is debugW315 to avoid ambiguity in shared namespace.)
interface W315Debuggable {
    function debugW315(self) -> string throws never
}
implements W315Debuggable for int {
    function debugW315(self) -> string { return "int" }
}
function w315_main() -> int {
    let a = reflect.Type.of<int>().implements(reflect.Type.of<W315Debuggable>())
    let b = reflect.Type.of<W315Debuggable>().implemented_by(reflect.Type.of<int>())
    if a && b { return 1001 }
    return 0
}
test "wf3_out_of_body_primitive_impl_visible_to_reflection_runtime" {
    assert.equal(w315_main(), 1001)
}

// wf3_self_interface_field_access_via_projection_runtime
interface W3spNamed {
    name: string
    function greet(self) -> string throws never
}
class W3spPerson {
    title: string
    implements W3spNamed {
        name as title
        function greet(self) -> string {
            return "Hi " + self.as<W3spNamed>.name
        }
    }
}
function w3sp_main() -> string {
    let p = W3spPerson { title: "Ada" }
    let n: W3spNamed = p
    return n.greet()
}
test "wf3_self_interface_field_access_via_projection_runtime" {
    assert.equal(w3sp_main(), "Hi Ada")
}

// wf3_out_of_body_method_callable_directly_runtime
// (Primitive method name is debugW3oo; direct unqualified `x.debugW3oo()` must
// resolve to exactly one interface method in the shared namespace.)
interface W3ooDebuggable { function debugW3oo(self) -> string throws never }
implements W3ooDebuggable for int {
    function debugW3oo(self) -> string { return "int" }
}
function w3oo_main() -> string {
    let x: int = 5
    return x.debugW3oo()
}
test "wf3_out_of_body_method_callable_directly_runtime" {
    assert.equal(w3oo_main(), "int")
}

// wf3_blanket_method_callable_directly_runtime
interface W3bmPrintable { function display(self) -> string throws never }
class W3bmBox<T> { value: T }
implements<T> W3bmPrintable for W3bmBox<T> {
    function display(self) -> string { return "box" }
}
function w3bm_main() -> string {
    let b = W3bmBox<int> { value: 42 }
    return b.display()
}
test "wf3_blanket_method_callable_directly_runtime" {
    assert.equal(w3bm_main(), "box")
}

// wf3_union_member_method_call_works_pins
interface W3umAnimal {
    function speak(self) -> string throws never
}
class W3umDog {
    implements W3umAnimal {
        function speak(self) -> string { return "Woof!" }
    }
}
class W3umCat {
    implements W3umAnimal {
        function speak(self) -> string { return "Meow." }
    }
}
function w3um_describe(x: W3umDog | W3umCat) -> string {
    return x.speak()
}
function w3um_main() -> string {
    let c: W3umCat = W3umCat {}
    return w3um_describe(c)
}
test "wf3_union_member_method_call_works_pins" {
    assert.equal(w3um_main(), "Meow.")
}

// wf3_concrete_field_shadows_interface_views_pins
interface W3cfNamed { name: string }
interface W3cfLabeled { name: string }
class W3cfItem {
    name: string
    other: string
    implements W3cfNamed {}
    implements W3cfLabeled { name as other }
}
function w3cf_main() -> string {
    let i = W3cfItem { name: "N", other: "O" }
    return i.name
}
test "wf3_concrete_field_shadows_interface_views_pins" {
    assert.equal(w3cf_main(), "N")
}

// wf3: generic-interface reflection is per-instantiation. A bare generic head
// (`reflect.Type.of<W3bgBox>()`) is an arity error like any other type
// position (pinned in tests/interfaces.rs); each written instantiation
// answers membership only for its own implementors.
interface W3bgBox<T> {
    function get(self) -> T throws never
}
class W3bgIntBox {
    implements W3bgBox<int> {
        function get(self) -> int { return 1 }
    }
}
class W3bgStringBox {
    implements W3bgBox<string> {
        function get(self) -> string { return "hi" }
    }
}
function w3bg_main() -> bool {
    let int_box = reflect.Type.of<W3bgBox<int>>()
    return int_box.implemented_by(reflect.Type.of<W3bgIntBox>())
        && !int_box.implemented_by(reflect.Type.of<W3bgStringBox>())
}
test "wf3_generic_interface_reflection_is_per_instantiation" {
    assert.is_true(w3bg_main())
}

// ─── union-fuzz findings + PR #3638 follow-ups ───────────────────────────────

// union_fuzz_pr_class_union_inherited_default_method
interface Ufp1Greeter {
    function greet(self) -> string throws never { return "hello" }
}
class Ufp1Dog { implements Ufp1Greeter {} }
class Ufp1Cat { implements Ufp1Greeter {} }
function ufp1_describe(x: Ufp1Dog | Ufp1Cat) -> string { return x.greet() }
function ufp1_main() -> string { return ufp1_describe(Ufp1Dog {}) + "/" + ufp1_describe(Ufp1Cat {}) }
test "union_fuzz_pr_class_union_inherited_default_method" {
    assert.equal(ufp1_main(), "hello/hello")
}

// union_fuzz_pr_reflection_nested_union_arg_order_insensitive
interface Ufp2Box<T> { function get(self) -> T throws never }
class Ufp2UBox {
    value: int
    implements Ufp2Box<(int | string)?> {
        function get(self) -> (int | string)? { return self.value }
    }
}
function ufp2_main() -> int {
    let ub = reflect.Type.of<Ufp2UBox>()
    let fwd = reflect.Type.of<Ufp2Box<(int | string)?>>()
    let rev = reflect.Type.of<Ufp2Box<(string | int)?>>()
    let a = 0
    if ub.implements(fwd) { a = a + 1 }
    if ub.implements(rev) { a = a + 10 }
    return a
}
test "union_fuzz_pr_reflection_nested_union_arg_order_insensitive" {
    assert.equal(ufp2_main(), 11)
}

// union_fuzz_pr_function_typed_interface_field_in_union
interface Ufp3HasHandler { handler: (int) -> string throws never }
class Ufp3A {
    handler: (int) -> string throws never
    label: string
    implements Ufp3HasHandler {}
}
function ufp3_getHandler(x: Ufp3HasHandler | Ufp3A) -> (int) -> string throws never { return x.handler }
function ufp3_main() -> string {
    let a: Ufp3HasHandler = Ufp3A { handler: (n: int) -> string { return "handled" }, label: "a" }
    let h = ufp3_getHandler(a)
    return h(5)
}
test "union_fuzz_pr_function_typed_interface_field_in_union" {
    assert.equal(ufp3_main(), "handled")
}

// union_fuzz_pr_optional_union_interface_field
interface Ufp4Named { name: string }
class Ufp4Dog { name: string  implements Ufp4Named {} }
function ufp4_readName(x: (Ufp4Dog | Ufp4Named)?) -> string? { return x?.name }
function ufp4_main() -> string? { return ufp4_readName(Ufp4Dog { name: "Rex" }) }
test "union_fuzz_pr_optional_union_interface_field" {
    assert.equal(ufp4_main(), "Rex")
}

// union_fuzz_pr_reflection_duplicate_union_members_not_equivalent
interface Ufp5Box<T> { function get(self) -> T throws never }
class Ufp5UBox {
    value: int
    implements Ufp5Box<int | int> { function get(self) -> int | int { return self.value } }
}
function ufp5_main() -> int {
    let ub = reflect.Type.of<Ufp5UBox>()
    let a = 0
    // UBox implements Box<int | int>, NOT Box<int | string>.
    if ub.implements(reflect.Type.of<Ufp5Box<int | int>>()) { a = a + 1 }
    if ub.implements(reflect.Type.of<Ufp5Box<int | string>>()) { a = a + 10 }
    return a
}
test "union_fuzz_pr_reflection_duplicate_union_members_not_equivalent" {
    assert.equal(ufp5_main(), 1)
}

// union_fuzz_f01_field_read_on_iface_union_member
interface Uf01Named {
    name: string
}
class Uf01Dog {
    name: string
    implements Uf01Named {}
}
function uf01_readName(x: Uf01Dog | Uf01Named) -> string {
    return x.name
}
function uf01_main() -> string {
    return uf01_readName(Uf01Dog { name: "Rex" })
}
test "union_fuzz_f01_field_read_on_iface_union_member" {
    assert.equal(uf01_main(), "Rex")
}

// union_fuzz_f02_generic_match_narrowing_ignores_type_arg_crash
interface Uf02Slot<T> { value: T }
class Uf02IntSlot {
    n: int
    implements Uf02Slot<int> { value as n }
}
class Uf02StrSlot {
    s: string
    implements Uf02Slot<string> { value as s }
}
function uf02_addOne(x: Uf02Slot<int> | Uf02Slot<string>) -> int {
    return match (x) {
        let a: Uf02Slot<int> => a.value + 1,
        let b: Uf02Slot<string> => 0,
    }
}
function uf02_main() -> int {
    let b: Uf02Slot<int> | Uf02Slot<string> = Uf02StrSlot { s: "not a number" }
    return uf02_addOne(b)
}
test "union_fuzz_f02_generic_match_narrowing_ignores_type_arg_crash" {
    assert.equal(uf02_main(), 0)
}

// union_fuzz_f05_generic_match_narrowing_wrong_result
interface Uf05Slot<T> { value: T }
class Uf05IntSlot { n: int  implements Uf05Slot<int> { value as n } }
class Uf05StrSlot { s: string  implements Uf05Slot<string> { value as s } }
function uf05_go(x: Uf05Slot<int> | Uf05Slot<string>) -> string {
    return match (x) {
        let a: Uf05Slot<int> => "int",
        _ => "other",
    }
}
function uf05_main() -> string {
    let b: Uf05Slot<int> | Uf05Slot<string> = Uf05StrSlot { s: "z" }
    return uf05_go(b)
}
test "union_fuzz_f05_generic_match_narrowing_wrong_result" {
    assert.equal(uf05_main(), "other")
}

// union_fuzz_f06_reflection_generic_union_arg_order_insensitive
interface Uf06Box<T> {
    function get(self) -> T throws never
}
class Uf06UBox {
    value: int
    implements Uf06Box<int | string> {
        function get(self) -> int | string { return self.value }
    }
}
function uf06_main() -> int {
    let ub = reflect.Type.of<Uf06UBox>()
    let fwd = reflect.Type.of<Uf06Box<int | string>>()
    let rev = reflect.Type.of<Uf06Box<string | int>>()
    let d1 = 0
    if ub.implements(fwd) { d1 = 1 }
    let d2 = 0
    if fwd.implemented_by(ub) { d2 = 1 }
    let d4 = 0
    if ub.implements(rev) { d4 = 1 }
    let d5 = 0
    if rev.implemented_by(ub) { d5 = 1 }
    return d1*1000 + d2*100 + d4*10 + d5
}
test "union_fuzz_f06_reflection_generic_union_arg_order_insensitive" {
    assert.equal(uf06_main(), 1111)
}

// union_fuzz_f08_interface_bounded_generic_identity_compiles
interface Uf08Animal {
    function speak(self) -> string throws never
}
class Uf08Dog {
    implements Uf08Animal {
        function speak(self) -> string { return "Woof!" }
    }
    function fetch(self) -> string { return "fetched" }
}
function uf08_identity<T extends Uf08Animal>(a: T) -> T {
    return a
}
function uf08_main() -> string {
    let d = uf08_identity<Uf08Dog>(Uf08Dog {})
    return d.fetch() + "/" + d.speak()
}
test "union_fuzz_f08_interface_bounded_generic_identity_compiles" {
    assert.equal(uf08_main(), "fetched/Woof!")
}

// union_fuzz_f09_interface_arm_over_optional_union_compiles
interface Uf09Animal {
    function speak(self) -> string throws never
}
class Uf09Dog {
    name: string
    implements Uf09Animal {
        function speak(self) -> string { return "Woof" }
    }
}
class Uf09Cat {
    implements Uf09Animal {
        function speak(self) -> string { return "Meow" }
    }
}
function uf09_main() -> string {
    let v: (Uf09Dog | Uf09Cat)? = Uf09Cat {}
    match (v) {
        let a: Uf09Animal => a.speak()
        null => "none"
    }
}
test "union_fuzz_f09_interface_arm_over_optional_union_compiles" {
    assert.equal(uf09_main(), "Meow")
}

// union_fuzz_f10_out_of_body_primitive_string_direct_call
// (Primitive method name is debugUf10 to avoid ambiguity in shared namespace.)
interface Uf10Debuggable { function debugUf10(self) -> string throws never }
implements Uf10Debuggable for string { function debugUf10(self) -> string { return "str" } }
function uf10_main() -> string {
    let s: string = "hi"
    return s.debugUf10()
}
test "union_fuzz_f10_out_of_body_primitive_string_direct_call" {
    assert.equal(uf10_main(), "str")
}

// ─── Group R: dispatch-guard and call-site type-arg regressions ──────────────

// positional_typevar_union_interface_args_dispatch
class PtuErrA { m: string }
class PtuErrB { m: string }
interface PtuPipe<E> {
    function run(self) -> string throws never
}
class PtuMulti<E, E2> {
    tag: string
    implements PtuPipe<E | E2> {
        function run(self) -> string { "multi:" + self.tag }
    }
}
function ptu_main() -> string {
    let p: PtuPipe<PtuErrA | PtuErrB> = PtuMulti<PtuErrA, PtuErrB> { tag: "x" };
    p.run()
}
test "positional_typevar_union_interface_args_dispatch" {
    assert.equal(ptu_main(), "multi:x")
}

// ── Ported from tests/interfaces.rs ──────────────────────────────────────────

// The dispatch cache keys on the *realized* interface arguments, not just the
// interface's nominal identity: two `Tagged<...>` instantiations on one class
// must select different implementations from the same fact-poor structural
// context.
interface VdcAnimal {}

class VdcDog {
    implements VdcAnimal {}
}

interface VdcTagged<T> {
    function tag(self) -> string throws never
}

class VdcBoth {
    implements VdcTagged<VdcAnimal> {
        function tag(self) -> string { return "animal" }
    }
    implements VdcTagged<VdcAnimal | VdcDog> {
        function tag(self) -> string { return "animal-or-dog" }
    }
}

function vdc_read_tag<T>(value: VdcTagged<T>) -> string {
    return value.tag()
}

function vdc_dispatch_distinguishes_instantiations() -> string {
    let value = VdcBoth {};
    return vdc_read_tag<VdcAnimal>(value) + "|" + vdc_read_tag<VdcAnimal | VdcDog>(value)
}

test "virtual_dispatch_cache_distinguishes_generic_interface_instantiations" {
    assert.equal(vdc_dispatch_distinguishes_instantiations(), "animal|animal-or-dog")
}
