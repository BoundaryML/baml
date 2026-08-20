// Blanket-implementation (Form 1 / Form 2) runtime tests and the fuzz-sweep
// regression suite. See interfaces.baml for the LEFT IN RUST list.

// ─── Blanket implementations — Form 1 runtime ────────────────────────────────

// form1_dispatches_through_interface_typed_var
interface F1dPrintable {
    function display(self) -> string throws never
}
class F1dBox<T> {
    value: T
}
implements<T> F1dPrintable for F1dBox<T> {
    function display(self) -> string { return "a box" }
}
function f1d_main() -> string {
    let b: F1dPrintable = F1dBox<int> { value: 42 }
    return b.display()
}
test "form1_dispatches_through_interface_typed_var" {
    assert.equal(f1d_main(), "a box")
}

// form1_dispatches_for_different_instantiations
interface F1diPrintable {
    function display(self) -> string throws never
}
class F1diBox<T> {
    value: T
}
implements<T> F1diPrintable for F1diBox<T> {
    function display(self) -> string { return "a box" }
}
function f1di_main() -> string {
    let a: F1diPrintable = F1diBox<int> { value: 42 }
    let b: F1diPrintable = F1diBox<string> { value: "hi" }
    return a.display() + " " + b.display()
}
test "form1_dispatches_for_different_instantiations" {
    assert.equal(f1di_main(), "a box a box")
}

// form1_self_accesses_receiver_fields
interface F1saDescribable {
    function describe(self) -> string throws never
}
class F1saPair<T> {
    first: T
    second: T
    label: string
}
implements<T> F1saDescribable for F1saPair<T> {
    function describe(self) -> string { return self.label }
}
function f1sa_main() -> string {
    let p: F1saDescribable = F1saPair<int> { first: 1, second: 2, label: "a pair" }
    return p.describe()
}
test "form1_self_accesses_receiver_fields" {
    assert.equal(f1sa_main(), "a pair")
}

// form1_with_generic_interface_args
interface F1gContainer<T> {
    function get(self) -> T throws never
}
class F1gWrapper<T> {
    value: T
}
implements<T> F1gContainer<T> for F1gWrapper<T> {
    function get(self) -> T { return self.value }
}
function f1g_main() -> int {
    let w: F1gContainer<int> = F1gWrapper<int> { value: 42 }
    return w.get()
}
test "form1_with_generic_interface_args" {
    assert.equal(f1g_main(), 42)
}

// generic_rule_for_list_receiver_dispatches
interface GrlrLabel {
    function label(self) -> string throws never
}
implements<T> GrlrLabel for T[] {
    function label(self) -> string { return "list" }
}
function grlr_main() -> string {
    let xs: int[] = [1, 2, 3]
    let labelled: GrlrLabel = xs
    return labelled.label()
}
test "generic_rule_for_list_receiver_dispatches" {
    assert.equal(grlr_main(), "list")
}

// generic_rule_for_map_receiver_dispatches
interface GrmrLabel {
    function label(self) -> string throws never
}
implements<T> GrmrLabel for map<string, T> {
    function label(self) -> string { return "map" }
}
function grmr_main() -> string {
    let values: map<string, int> = { "a": 1 }
    let labelled: GrmrLabel = values
    return labelled.label()
}
test "generic_rule_for_map_receiver_dispatches" {
    assert.equal(grmr_main(), "map")
}

// unified_rule_nested_interface_args_dispatch
interface UrniContainer<T> {
    function get(self) -> T throws never
}
class UrniWrapper<T> {
    values: T[]
}
implements<T> UrniContainer<T[]> for UrniWrapper<T> {
    function get(self) -> T[] { return self.values }
}
function urni_main() -> int {
    let w: UrniContainer<int[]> = UrniWrapper<int> { values: [1, 2, 3] }
    return w.get().length()
}
test "unified_rule_nested_interface_args_dispatch" {
    assert.equal(urni_main(), 3)
}

// unified_rule_repeated_type_vars_match_runtime
interface UrrtSame {
    function tag(self) -> string throws never
}
class UrrtPair<L, R> {
    left: L
    right: R
}
implements<T> UrrtSame for UrrtPair<T, T> {
    function tag(self) -> string { return "same" }
}
function urrt_main() -> string {
    let p: UrrtSame = UrrtPair<int, int> { left: 7, right: 8 }
    return p.tag()
}
test "unified_rule_repeated_type_vars_match_runtime" {
    assert.equal(urrt_main(), "same")
}

// unified_rule_default_method_inherited_through_generic_rule
interface UrdmPrintable {
    function display(self) -> string throws never { return "default" }
}
class UrdmBox<T> {
    value: T
}
implements<T> UrdmPrintable for UrdmBox<T> {}
function urdm_main() -> string {
    let b: UrdmPrintable = UrdmBox<int> { value: 42 }
    return b.display()
}
test "unified_rule_default_method_inherited_through_generic_rule" {
    assert.equal(urdm_main(), "default")
}

// unified_rule_reflection_sees_generic_class_implementor_once
interface UrrsPrintable {
    function display(self) -> string throws never
}
class UrrsBox<T> {
    value: T
}
implements<T> UrrsPrintable for UrrsBox<T> {
    function display(self) -> string { return "box" }
}
function urrs_main() -> bool {
    return reflect.Type.of<UrrsBox<int>>().implements(reflect.Type.of<UrrsPrintable>())
        && reflect.Type.of<UrrsBox<string>>().implements(reflect.Type.of<UrrsPrintable>())
}
test "unified_rule_reflection_sees_generic_class_implementor_once" {
    assert.is_true(urrs_main())
}

// ─── Blanket implementations — Form 2 runtime ────────────────────────────────

// form2_dispatches_through_interface_typed_var
interface F2dNamed {
    name: string
}
interface F2dPrintable {
    function display(self) -> string throws never
}
class F2dPerson {
    name: string
    implements F2dNamed {}
}
implements<T extends F2dNamed> F2dPrintable for T {
    function display(self) -> string { return "named:" + self.name }
}
function f2d_main() -> string {
    let p: F2dPrintable = F2dPerson { name: "Ada" }
    return p.display()
}
test "form2_dispatches_through_interface_typed_var" {
    assert.equal(f2d_main(), "named:Ada")
}

// form2_self_accesses_bound_members
interface F2saNamed {
    name: string
}
interface F2saLabeled {
    function label(self) -> string throws never
}
class F2saProject {
    name: string
    implements F2saNamed {}
}
implements<T extends F2saNamed> F2saLabeled for T {
    function label(self) -> string { return self.name }
}
function f2sa_main() -> string {
    let item: F2saLabeled = F2saProject { name: "Launch" }
    return item.label()
}
test "form2_self_accesses_bound_members" {
    assert.equal(f2sa_main(), "Launch")
}

// form2_applies_to_multiple_satisfying_classes
interface F2amNamed {
    name: string
}
interface F2amPrintable {
    function display(self) -> string throws never
}
class F2amPerson {
    name: string
    implements F2amNamed {}
}
class F2amTeam {
    name: string
    implements F2amNamed {}
}
implements<T extends F2amNamed> F2amPrintable for T {
    function display(self) -> string { return self.name }
}
function f2am_main() -> string {
    let person: F2amPrintable = F2amPerson { name: "Ada" }
    let team: F2amPrintable = F2amTeam { name: "Core" }
    return person.display() + "/" + team.display()
}
test "form2_applies_to_multiple_satisfying_classes" {
    assert.equal(f2am_main(), "Ada/Core")
}

// form2_reflect_implements_returns_true
interface F2riNamed {
    name: string
}
interface F2riPrintable {
    function display(self) -> string throws never
}
class F2riPerson {
    name: string
    implements F2riNamed {}
}
implements<T extends F2riNamed> F2riPrintable for T {
    function display(self) -> string { return self.name }
}
function f2ri_main() -> bool {
    return reflect.Type.of<F2riPerson>().implements(reflect.Type.of<F2riPrintable>())
}
test "form2_reflect_implements_returns_true" {
    assert.is_true(f2ri_main())
}

// form2_reflect_membership_includes_satisfying_classes
interface F2rsNamed {
    name: string
}
interface F2rsPrintable {
    function display(self) -> string throws never
}
class F2rsPerson {
    name: string
    implements F2rsNamed {}
}
class F2rsTeam {
    name: string
    implements F2rsNamed {}
}
implements<T extends F2rsNamed> F2rsPrintable for T {
    function display(self) -> string { return self.name }
}
function f2rs_main() -> bool {
    let printable = reflect.Type.of<F2rsPrintable>()
    return printable.implemented_by(reflect.Type.of<F2rsPerson>())
        && printable.implemented_by(reflect.Type.of<F2rsTeam>())
}
test "form2_reflect_membership_includes_satisfying_classes" {
    assert.is_true(f2rs_main())
}

// blanket_interface_rule_reflection_includes_concrete_container_implementors
interface BirrPrintable<T> {}
class BirrContainer<T> {
    value: T
}
implements<T> BirrPrintable<T> for BirrContainer<T> {}
function birr_main() -> bool {
    let int_printable: BirrPrintable<int> = BirrContainer<int> { value: 1 }
    let string_printable: BirrPrintable<string> = BirrContainer<string> { value: "two" }
    return reflect.Type.of<BirrContainer<int>>().implements(reflect.Type.of<BirrPrintable<int>>())
        && reflect.Type.of<BirrContainer<string>>().implements(reflect.Type.of<BirrPrintable<string>>())
        && reflect.Type.of<BirrPrintable<int>>().implemented_by(reflect.Type.of<BirrContainer<int>>())
        && reflect.Type.of<BirrPrintable<string>>().implemented_by(reflect.Type.of<BirrContainer<string>>())
        && !reflect.Type.of<BirrPrintable<int>>().implemented_by(reflect.Type.of<BirrContainer<string>>())
        && !reflect.Type.of<BirrPrintable<string>>().implemented_by(reflect.Type.of<BirrContainer<int>>())
}
test "blanket_interface_rule_reflection_includes_concrete_container_implementors" {
    assert.is_true(birr_main())
}

// unified_rule_implementor_satisfies_generic_interface_bound
interface UrisPrintable {
    function display(self) -> string throws never
}
class UrisBox<T> {
    value: T
}
implements<T> UrisPrintable for UrisBox<T> {
    function display(self) -> string { return "box" }
}
function uris_f<T extends UrisPrintable>(x: T) -> string {
    return x.display()
}
function uris_main() -> string {
    return uris_f<UrisBox<int>>(UrisBox<int> { value: 42 })
}
test "unified_rule_implementor_satisfies_generic_interface_bound" {
    assert.equal(uris_main(), "box")
}

// unified_rule_implementor_satisfies_inferred_generic_interface_bound
interface UriiPrintable {
    function display(self) -> string throws never
}
class UriiBox<T> {
    value: T
}
implements<T> UriiPrintable for UriiBox<T> {
    function display(self) -> string { return "box" }
}
function urii_f<T extends UriiPrintable>(x: T) -> string {
    return x.display()
}
function urii_main() -> string {
    return urii_f(UriiBox<int> { value: 42 })
}
test "unified_rule_implementor_satisfies_inferred_generic_interface_bound" {
    assert.equal(urii_main(), "box")
}

// unified_rule_implementor_satisfies_inferred_generic_bound_from_constructor_args
interface UricPrintable {
    function display(self) -> string throws never
}
class UricBox<T> {
    value: T
}
implements<T> UricPrintable for UricBox<T> {
    function display(self) -> string { return "box" }
}
function uric_f<T extends UricPrintable>(x: T) -> string {
    return x.display()
}
function uric_main() -> string {
    return uric_f(UricBox { value: 42 })
}
test "unified_rule_implementor_satisfies_inferred_generic_bound_from_constructor_args" {
    assert.equal(uric_main(), "box")
}

// bounded_type_var_rule_satisfies_generic_interface_bound
interface BtvsNamed {
    name: string
}
interface BtvsPrintable {
    function display(self) -> string throws never
}
class BtvsPerson {
    name: string
    implements BtvsNamed {}
}
implements<T extends BtvsNamed> BtvsPrintable for T {
    function display(self) -> string { return self.name }
}
function btvs_f<T extends BtvsPrintable>(x: T) -> string {
    return x.display()
}
function btvs_main() -> string {
    return btvs_f<BtvsPerson>(BtvsPerson { name: "Ada" })
}
test "bounded_type_var_rule_satisfies_generic_interface_bound" {
    assert.equal(btvs_main(), "Ada")
}

// bounded_type_var_rule_satisfies_inferred_generic_interface_bound
interface BtviNamed {
    name: string
}
interface BtviPrintable {
    function display(self) -> string throws never
}
class BtviPerson {
    name: string
    implements BtviNamed {}
}
implements<T extends BtviNamed> BtviPrintable for T {
    function display(self) -> string { return self.name }
}
function btvi_f<T extends BtviPrintable>(x: T) -> string {
    return x.display()
}
function btvi_main() -> string {
    return btvi_f(BtviPerson { name: "Ada" })
}
test "bounded_type_var_rule_satisfies_inferred_generic_interface_bound" {
    assert.equal(btvi_main(), "Ada")
}

// form1_bounded_generic_receiver_dispatches_when_bound_satisfied
interface F1bgNamed {
    name: string
}
interface F1bgPrintable {
    function display(self) -> string throws never
}
class F1bgPerson {
    name: string
    implements F1bgNamed {}
}
class F1bgWrapper<T> {
    inner: T
}
implements<T extends F1bgNamed> F1bgPrintable for F1bgWrapper<T> {
    function display(self) -> string { return self.inner.name }
}
function f1bg_main() -> string {
    let item: F1bgPrintable = F1bgWrapper<F1bgPerson> { inner: F1bgPerson { name: "Ada" } }
    return item.display()
}
test "form1_bounded_generic_receiver_dispatches_when_bound_satisfied" {
    assert.equal(f1bg_main(), "Ada")
}

// unified_rule_requires_closure_preserves_substituted_generic_args
interface UrrcParent<T> {
    function get(self) -> T throws never
}
interface UrrcChild<T> requires UrrcParent<T> {}
class UrrcWrapper<T> {
    value: T
}
implements<T> UrrcParent<T> for UrrcWrapper<T> {
    function get(self) -> T { return self.value }
}
implements<T> UrrcChild<T> for UrrcWrapper<T> {}
function urrc_take(child: UrrcChild<int>) -> int {
    return child.get()
}
function urrc_main() -> int {
    return urrc_take(UrrcWrapper<int> { value: 42 })
}
test "unified_rule_requires_closure_preserves_substituted_generic_args" {
    assert.equal(urrc_main(), 42)
}

// ─── BEP-044 fuzz-sweep regression suite ─────────────────────────────────────

// fuzz_bug03_implementor_assignable_to_optional_interface_param
interface Fz03Animal {
    function speak(self) -> string throws never
}
class Fz03Dog {
    implements Fz03Animal {
        function speak(self) -> string { return "Woof!" }
    }
}
function fz03_accepts_optional(a: Fz03Animal?) -> string {
    if (a != null) {
        return a.speak()
    } else {
        return "none"
    }
}
function fz03_main() -> string {
    let d = Fz03Dog {}
    return fz03_accepts_optional(d)
}
test "fuzz_bug03_implementor_assignable_to_optional_interface_param" {
    assert.equal(fz03_main(), "Woof!")
}

// fuzz_bug04_method_call_on_parenthesized_if_expr
interface Fz04Animal {
    function speak(self) -> string throws never
}
class Fz04Dog {
    implements Fz04Animal {
        function speak(self) -> string { return "Woof!" }
    }
}
class Fz04Cat {
    implements Fz04Animal {
        function speak(self) -> string { return "Meow." }
    }
}
function fz04_main() -> string {
    return (if (true) { Fz04Dog {} } else { Fz04Cat {} }).speak()
}
test "fuzz_bug04_method_call_on_parenthesized_if_expr" {
    assert.equal(fz04_main(), "Woof!")
}

// fuzz_bug05_requires_child_override_does_not_leak_into_parent_slot
interface Fz05A {
    function foo(self) -> string throws never { return "A" }
}
interface Fz05B requires Fz05A {
    function foo(self) -> string throws never { return "B" }
}
class Fz05C {
    implements Fz05A {}
    implements Fz05B {
        function foo(self) -> string { return "B-override" }
    }
}
function fz05_main() -> string {
    let c = Fz05C {}
    let a: Fz05A = c
    return a.foo()
}
test "fuzz_bug05_requires_child_override_does_not_leak_into_parent_slot" {
    assert.equal(fz05_main(), "A")
}

// fuzz_bug07_generic_default_method_body_can_use_type_param
interface Fz07Echo<T> {
    function echo(self, x: T) -> T throws never {
        return x
    }
}
class Fz07IntEcho {
    implements Fz07Echo<int> {}
}
function fz07_main() -> int {
    let e = Fz07IntEcho {}
    return e.echo(42)
}
test "fuzz_bug07_generic_default_method_body_can_use_type_param" {
    assert.equal(fz07_main(), 42)
}

// fuzz_bug10_generic_interface_as_function_param_dispatches
interface Fz10Box<T> {
    function get(self) -> T throws never
}
class Fz10IntBox {
    value: int
    implements Fz10Box<int> {
        function get(self) -> int { return self.value }
    }
}
// Function parameter type is Fz10Box<T> (generic interface)
function fz10_read<T>(b: Fz10Box<T>) -> T {
    return b.get()
}
function fz10_main() -> int {
    let b = Fz10IntBox { value: 55 }
    return fz10_read<int>(b)
}
test "fuzz_bug10_generic_interface_as_function_param_dispatches" {
    assert.equal(fz10_main(), 55)
}

// fuzz_bug15_generic_default_method_self_call_through_interface_var
interface Fz15Container<T> {
    function size(self) -> int throws never
    function describe_with_self_call(self) -> string throws never {
        let n = self.size()   // calling another interface method on self
        return "ok"
    }
}
class Fz15IntBox {
    items: int[]
    implements Fz15Container<int> {
        function size(self) -> int { return self.items.length() }
    }
}
function fz15_main() -> string {
    let b = Fz15IntBox { items: [1, 2, 3] }
    let c: Fz15Container<int> = b
    return c.describe_with_self_call()
}
test "fuzz_bug15_generic_default_method_self_call_through_interface_var" {
    assert.equal(fz15_main(), "ok")
}

// fuzz_bug16_generic_interface_type_param_in_scope_in_default_method
interface Fz16Container<T> {
    function get(self) -> T throws never           // required method - T is fine here
    function get_or(self, fallback: T) -> T throws never {   // default method - T must be in scope
        return self.get()
    }
}
class Fz16IntBox {
    value: int
    implements Fz16Container<int> {
        function get(self) -> int { return self.value }
    }
}
function fz16_main() -> bool {
    return true
}
test "fuzz_bug16_generic_interface_type_param_in_scope_in_default_method" {
    assert.is_true(fz16_main())
}

// fuzz_bug17_inherited_default_method_callable_on_class_var
interface Fz17Greetable {
    function greet(self) -> string throws never {
        return "Hello!"
    }
}
class Fz17Greeter {
    implements Fz17Greetable {}  // empty block - inherits default
}
function fz17_main() -> string {
    let g = Fz17Greeter {}
    return g.greet()  // should work - inherits default
}
test "fuzz_bug17_inherited_default_method_callable_on_class_var" {
    assert.equal(fz17_main(), "Hello!")
}

// fuzz_bug18_class_method_preferred_over_aliased_field_view_on_call
interface Fz18Named {
    name: string
}
class Fz18Person {
    _name: string
    implements Fz18Named {
        name as _name
    }
    function name(self) -> string { return "method:" + self._name }
}
function fz18_main() -> string {
    let p = Fz18Person { _name: "Ada" }
    return p.name()  // Should call the class method
}
test "fuzz_bug18_class_method_preferred_over_aliased_field_view_on_call" {
    assert.equal(fz18_main(), "method:Ada")
}

// fuzz_bug20_as_projection_field_uses_own_view_in_requires_chain
interface Fz20A {
    label: string
}
interface Fz20B requires Fz20A {
    label: string
}
class Fz20D {
    a_label: string
    b_label: string
    implements Fz20A { label as a_label }
    implements Fz20B { label as b_label }
}
function fz20_main() -> string {
    let d = Fz20D { a_label: "A_val", b_label: "B_val" }
    let a_val = d.as<Fz20A>.label
    let b_val = d.as<Fz20B>.label
    return a_val + "|" + b_val
}
test "fuzz_bug20_as_projection_field_uses_own_view_in_requires_chain" {
    assert.equal(fz20_main(), "A_val|B_val")
}

// fuzz_bug21_interface_param_field_uses_own_view_in_requires_chain
interface Fz21A {
    label: string
}
interface Fz21B requires Fz21A {
    label: string
}
class Fz21D {
    a_label: string
    b_label: string
    implements Fz21A { label as a_label }
    implements Fz21B { label as b_label }
}
function fz21_get_from_b(b: Fz21B) -> string {
    return b.label
}
function fz21_get_from_a(a: Fz21A) -> string {
    return a.label
}
function fz21_main() -> string {
    let d = Fz21D { a_label: "A_val", b_label: "B_val" }
    return fz21_get_from_a(d) + "|" + fz21_get_from_b(d)
}
test "fuzz_bug21_interface_param_field_uses_own_view_in_requires_chain" {
    assert.equal(fz21_main(), "A_val|B_val")
}

// fuzz_bug23_default_inherited_method_unqualified_call_on_class
interface Fz23Speaker {
    function speak(self) -> string throws never {
        return "default speech"
    }
}
class Fz23Thing {
    implements Fz23Speaker {}
}
function fz23_main() -> string {
    let t = Fz23Thing {}
    return t.speak()
}
test "fuzz_bug23_default_inherited_method_unqualified_call_on_class" {
    assert.equal(fz23_main(), "default speech")
}

// fuzz_bug24_generic_interface_type_param_in_default_method_signature
interface Fz24Container<T> {
    // Default method using the interface's type parameter `T` in both its
    // signature and body — `T` must be in scope here.
    function identity(self, x: T) -> T throws never {
        return x
    }
    function get(self) -> T throws never  // required
}
class Fz24IntBag {
    items: int[]
    implements Fz24Container<int> {
        function get(self) -> int { return self.items[0] }
    }
}
function fz24_main() -> int {
    let b: Fz24Container<int> = Fz24IntBag { items: [42, 1, 2] }
    return b.get()
}
test "fuzz_bug24_generic_interface_type_param_in_default_method_signature" {
    assert.equal(fz24_main(), 42)
}

// fuzz_bug29_match_binding_form_narrows_to_concrete
interface Fz29Animal {
    function speak(self) -> string throws never
}
class Fz29Dog {
    breed: string
    implements Fz29Animal {
        function speak(self) -> string { return "Woof!" }
    }
}
function fz29_describe(a: Fz29Animal) -> string {
    return match (a) {
        let d: Fz29Dog => d.breed
        _ => "other"
    }
}
function fz29_main() -> string {
    let d = Fz29Dog { breed: "Retriever" }
    return fz29_describe(d)
}
test "fuzz_bug29_match_binding_form_narrows_to_concrete" {
    assert.equal(fz29_main(), "Retriever")
}

// fuzz_bug31_method_call_on_match_union_result_does_not_crash
interface Fz31Animal {
    function speak(self) -> string throws never
}
class Fz31Dog {
    implements Fz31Animal {
        function speak(self) -> string { return "Woof!" }
    }
}
class Fz31Cat {
    implements Fz31Animal {
        function speak(self) -> string { return "Meow!" }
    }
}
function fz31_main() -> string {
    let b = true
    let result = match (b) {
        true => Fz31Dog {}
        false => Fz31Cat {}
    }
    // result is a Dog | Cat union; `speak()` is present on every member, so
    // the call dispatches on the runtime class (Dog here).
    return result.speak()
}
test "fuzz_bug31_method_call_on_match_union_result_does_not_crash" {
    assert.equal(fz31_main(), "Woof!")
}

// fuzz_bug32_interface_value_coerces_to_optional_interface
interface Fz32Animal {
    function speak(self) -> string throws never
}
class Fz32Dog {
    breed: string
    implements Fz32Animal {
        function speak(self) -> string { return "Woof!" }
    }
}
function fz32_main() -> bool {
    let d: Fz32Animal = Fz32Dog { breed: "Lab" }
    let opt: Fz32Animal? = d   // should work: Animal is a subtype of Animal?
    return opt != null
}
test "fuzz_bug32_interface_value_coerces_to_optional_interface" {
    assert.is_true(fz32_main())
}

// fuzz_bug33_return_implementor_from_optional_interface_function
interface Fz33Animal {
    function speak(self) -> string throws never
}
class Fz33Dog {
    breed: string
    implements Fz33Animal {
        function speak(self) -> string { return "Woof!" }
    }
}
// Should work: Dog <: Animal <: Animal?
function fz33_maybe_dog(want_dog: bool) -> Fz33Animal? {
    if want_dog {
        return Fz33Dog { breed: "Lab" }
    }
    return null
}
function fz33_main() -> bool {
    let r = fz33_maybe_dog(true)
    let n = fz33_maybe_dog(false)
    return r != null && n == null
}
test "fuzz_bug33_return_implementor_from_optional_interface_function" {
    assert.is_true(fz33_main())
}

// fuzz_bug34_reflect_implements_respects_generic_type_args
interface Fz34Box<T> {
    function get(self) -> T throws never
}
class Fz34IntBox {
    implements Fz34Box<int> {
        function get(self) -> int { return 1 }
    }
}
function fz34_main() -> bool {
    // IntBox only implements Box<int>, NOT Box<string>, so this is false —
    // `implements` respects the generic type argument.
    return reflect.Type.of<Fz34IntBox>().implements(reflect.Type.of<Fz34Box<string>>())
}
test "fuzz_bug34_reflect_implements_respects_generic_type_args" {
    assert.is_true(fz34_main() == false)
}

// fuzz_bug35_reflect_implemented_by_respects_generic_type_args
interface Fz35Box<T> {
    function get(self) -> T throws never
}
class Fz35IntBox {
    implements Fz35Box<int> {
        function get(self) -> int { return 1 }
    }
}
function fz35_main() -> bool {
    // Box<string>.implemented_by(IntBox) is false — IntBox only implements
    // Box<int>, and `implemented_by` respects the type argument.
    return reflect.Type.of<Fz35Box<string>>().implemented_by(reflect.Type.of<Fz35IntBox>())
}
test "fuzz_bug35_reflect_implemented_by_respects_generic_type_args" {
    assert.is_true(fz35_main() == false)
}

// fuzz_bug38_throw_subtype_of_declared_throws_interface_is_allowed
interface Fz38IError {
    function describe(self) -> string throws never
}
class Fz38NetworkError {
    msg: string
    implements Fz38IError {
        function describe(self) -> string { return "network: " + self.msg }
    }
}
class Fz38DatabaseError {
    msg: string
    implements Fz38IError {
        function describe(self) -> string { return "db: " + self.msg }
    }
}
interface Fz38DataFetcher {
    function fetch(self, key: string) -> string throws Fz38IError
}
class Fz38MyFetcher {
    implements Fz38DataFetcher {
        function fetch(self, key: string) -> string throws Fz38IError {
            if key == "net" {
                throw Fz38NetworkError { msg: "connection refused" }
            }
            return "data:" + key
        }
    }
}
function fz38_main() -> bool {
    return true
}
test "fuzz_bug38_throw_subtype_of_declared_throws_interface_is_allowed" {
    assert.is_true(fz38_main())
}

// fuzz_bug39_catch_by_interface_pattern_matches_implementor
interface Fz39IError {
    function describe(self) -> string throws never
}
class Fz39ConcreteErr {
    msg: string
    implements Fz39IError {
        function describe(self) -> string { return self.msg }
    }
}
function fz39_risky() -> string throws Fz39IError {
    let e: Fz39IError = Fz39ConcreteErr { msg: "problem" }
    throw e
}
function fz39_main() -> string {
    return fz39_risky() catch (e) {
        let caught: Fz39IError => "interface-caught: " + caught.describe()
        _ => "wildcard-caught"
    }
}
test "fuzz_bug39_catch_by_interface_pattern_matches_implementor" {
    assert.equal(fz39_main(), "interface-caught: problem")
}

// fuzz_bug40_implementor_assignable_to_interface_union
interface Fz40Animal {
    function speak(self) -> string throws never
}
class Fz40Dog {
    implements Fz40Animal {
        function speak(self) -> string { return "Woof!" }
    }
}
function fz40_describe(x: Fz40Animal | string) -> string {
    return "ok"
}
function fz40_main() -> string {
    return fz40_describe(Fz40Dog {})
}
test "fuzz_bug40_implementor_assignable_to_interface_union" {
    assert.equal(fz40_main(), "ok")
}

// fuzz_bug41_optional_interface_match_null_arm_reachable
interface Fz41Animal {
    function speak(self) -> string throws never
}
class Fz41Dog {
    implements Fz41Animal {
        function speak(self) -> string { return "Woof!" }
    }
}
// Should require null arm for exhaustiveness; without it should be a compile error.
// With it, null arm should NOT be unreachable.
function fz41_maybe_speak(a: Fz41Animal?) -> string {
    return match (a) {
        let animal: Fz41Animal => animal.speak()
        null => "silent"
    }
}
function fz41_main() -> string {
    return fz41_maybe_speak(null)
}
test "fuzz_bug41_optional_interface_match_null_arm_reachable" {
    assert.equal(fz41_main(), "silent")
}

// fuzz_bug42_default_method_callable_on_concrete_class_type
interface Fz42Printable {
    function print(self) -> string throws never { return "printable" }
}
class Fz42Widget {
    implements Fz42Printable {}
}
function fz42_main() -> string {
    let w = Fz42Widget {}
    // Widget inherits Printable.print via empty implements block
    return w.print()
}
test "fuzz_bug42_default_method_callable_on_concrete_class_type" {
    assert.equal(fz42_main(), "printable")
}

// fuzz_bug43_generic_fn_with_generic_interface_param_dispatches
interface Fz43Producer<T> {
    function produce(self) -> T throws never
}
class Fz43IntProducer {
    val: int
    implements Fz43Producer<int> {
        function produce(self) -> int { return self.val }
    }
}
// T is unconstrained but used as the type arg in Producer<T>
function fz43_get_value<T>(p: Fz43Producer<T>) -> T {
    return p.produce()
}
function fz43_main() -> int {
    let p = Fz43IntProducer { val: 42 }
    return fz43_get_value<int>(p)
}
test "fuzz_bug43_generic_fn_with_generic_interface_param_dispatches" {
    assert.equal(fz43_main(), 42)
}

// fuzz_bug44_generic_interface_default_method_body_uses_type_param
interface Fz44Wrapper<T> {
    function wrap(self, x: T) -> T throws never {
        return x
    }
}
class Fz44IntWrapper {
    implements Fz44Wrapper<int> {}
}
function fz44_main() -> int {
    let w = Fz44IntWrapper {}
    return w.wrap(42)
}
test "fuzz_bug44_generic_interface_default_method_body_uses_type_param" {
    assert.equal(fz44_main(), 42)
}

// fuzz_bug45_interface_union_match_arms_all_reachable
interface Fz45Animal {
    function speak(self) -> string throws never
}
class Fz45Dog {
    implements Fz45Animal {
        function speak(self) -> string { return "Woof!" }
    }
}
function fz45_describe(x: Fz45Animal | string) -> string {
    return match (x) {
        let a: Fz45Animal => a.speak()
        let s: string => s
    }
}
function fz45_main() -> string {
    let a: Fz45Animal = Fz45Dog {}
    return fz45_describe(a)
}
test "fuzz_bug45_interface_union_match_arms_all_reachable" {
    assert.equal(fz45_main(), "Woof!")
}
