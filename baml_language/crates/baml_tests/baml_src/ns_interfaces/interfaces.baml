// Runtime behavior of `interface` declarations and `implements` blocks (BEP-044).
// See also interfaces_2.baml and interfaces_3.baml.
//
// Test programs are implemented as top-level helper functions; test blocks
// call these functions (workaround for test-block local-boxing VM bug).
// Because all programs share one namespace, types and functions get unique
// prefixes (e.g. `Animal` -> `ApwuAnimal` for test as_projection_works_when_unambiguous_runtime).
// Primitive method names must be unique (e.g. `debug` -> `debugOobp`)
// to avoid ambiguity in unqualified method lookup.
// Reflection `.to_string()` output is namespace-qualified ("interfaces.<Name>").
//
// LEFT IN RUST (crates/baml_tests/tests/interfaces.rs):
//   - all assert_compile_error_* / assert_no_compile_errors* /
//     assert_no_interface_errors / assert_zero_compile_errors tests
//     (diagnostics infrastructure: they assert on compiler diagnostics via
//     setup_test_db / ProjectDatabase + collect_diagnostics — presence or
//     absence of specific error codes/messages — not expressible as corpus
//     test blocks)
//   - aliased_interface_fields_do_not_create_concrete_runtime_slots and
//     interface_return_uses_concrete_implementor_field_shape (assert on the
//     host-side BexExternalValue::Instance field map shape across the FFI
//     boundary — that interface field views have no concrete runtime slots in
//     the serialized instance — not observable from inside BAML)
//   - fuzz_bug01_method_ref_required_method_crashes and
//     fuzz_bug02_method_ref_default_dispatches_to_override (#[ignore]d exec
//     tests pinning unimplemented behavior: interface methods as first-class
//     values with dynamic dispatch need a synthesized dispatcher thunk)

// ─── Group L: interface field namespaces ─────────────────────────────────────

// interface_fields_constructed_with_class_keys
interface IfccConfig {
    host: string
    port: int
}
class IfccServer {
    host: string
    port: int
    max_connections: int
    implements IfccConfig {}
}
function ifcc_main() -> bool {
    let s = IfccServer { host: "localhost", port: 8080, max_connections: 50 }
    let c: IfccConfig = s
    return c.host == "localhost"
        && c.port == 8080
        && s.host == "localhost"
        && s.max_connections == 50
}
test "interface_fields_constructed_with_class_keys" {
    assert.is_true(ifcc_main())
}

// class_own_field_shadowing_interface_field_is_separate_at_runtime
interface CofsConfig {
    host: string
}
class CofsServer {
    host: int
    config_host: string
    implements CofsConfig {
        host as config_host
    }
}
function cofs_main() -> bool {
    let s = CofsServer { host: 0, config_host: "localhost" }
    let c: CofsConfig = s
    return s.host == 0 && c.host == "localhost"
}
test "class_own_field_shadowing_interface_field_is_separate_at_runtime" {
    assert.is_true(cofs_main())
}

// two_interfaces_same_field_not_merged_runtime
interface TisfNamed { name: string }
interface TisfLabeled { name: string }
class TisfItem {
    named_name: string
    labeled_name: string
    implements TisfNamed { name as named_name }
    implements TisfLabeled { name as labeled_name }
}
function tisf_main() -> bool {
    let i = TisfItem { named_name: "widget", labeled_name: "WIDGET-001" }
    let n: TisfNamed = i
    let l: TisfLabeled = i
    return n.name == "widget" && l.name == "WIDGET-001"
}
test "two_interfaces_same_field_not_merged_runtime" {
    assert.is_true(tisf_main())
}

// same_field_name_different_interface_types_not_conflicting_runtime
interface SfndHasId { id: string }
interface SfndHasNumId { id: int }
class SfndThing {
    text_id: string
    numeric_id: int
    implements SfndHasId { id as text_id }
    implements SfndHasNumId { id as numeric_id }
}
function sfnd_main() -> bool {
    let t = SfndThing { text_id: "abc", numeric_id: 42 }
    let text: SfndHasId = t
    let numeric: SfndHasNumId = t
    return text.id == "abc" && numeric.id == 42
}
test "same_field_name_different_interface_types_not_conflicting_runtime" {
    assert.is_true(sfnd_main())
}

// same_generic_interface_field_links_select_matching_type_args_runtime
interface SgifSlot<T> {
    value: T
}
class SgifPair {
    int_value: int
    string_value: string
    implements SgifSlot<int> {
        value as int_value
    }
    implements SgifSlot<string> {
        value as string_value
    }
}
function sgif_main() -> bool {
    let p = SgifPair { int_value: 7, string_value: "seven" }
    let i: SgifSlot<int> = p
    let s: SgifSlot<string> = p
    return i.value == 7 && s.value == "seven"
}
test "same_generic_interface_field_links_select_matching_type_args_runtime" {
    assert.is_true(sgif_main())
}

// symbolic_interface_view_selects_per_instantiation_link_runtime
// The view is the frame's `T`, not anything on the receiver: both calls pass the
// SAME object, so no runtime test of the receiver can tell the two links apart.
// Only carrying the requested view to the resolver discriminates them.
function sgif_read<T>(g: SgifSlot<T>) -> T {
    g.value
}
function sgif_symbolic_main() -> bool {
    let p = SgifPair { int_value: 7, string_value: "seven" }
    return sgif_read<int>(p) == 7 && sgif_read<string>(p) == "seven"
}
test "symbolic_interface_view_selects_per_instantiation_link_runtime" {
    assert.is_true(sgif_symbolic_main())
}

// interface_field_write_through_existential_runtime
interface IfwCounter {
    count: int
}
class IfwBox {
    label: string
    count: int
    implements IfwCounter {}
}
function ifw_main() -> bool {
    let b = IfwBox { label: "b", count: 1 }
    let c: IfwCounter = b
    c.count = 10
    let after_assign = b.count == 10
    c.count += 5
    return after_assign && b.count == 15
}
test "interface_field_write_through_existential_runtime" {
    assert.is_true(ifw_main())
}

// interface_field_write_through_bounded_type_var_runtime
function ifw_bump<T extends IfwCounter>(t: T) -> void {
    t.count += 100
}
function ifw_typevar_main() -> bool {
    let b = IfwBox { label: "b", count: 1 }
    ifw_bump<IfwBox>(b)
    return b.count == 101
}
test "interface_field_write_through_bounded_type_var_runtime" {
    assert.is_true(ifw_typevar_main())
}

// generic_implementor_field_resolves_from_receiver_runtime
// Nothing at the access site names the implementor: `gif_read` sees only the
// existential. The slot comes from the receiver's own impl at run time, so a
// generic class works at every instantiation without any of them being
// enumerable at the access site.
interface GifHolder {
    held: string
}
class GifBox<T> {
    payload: T
    held: string
    implements GifHolder {}
}
function gif_read(h: GifHolder) -> string {
    h.held
}
function gif_main() -> bool {
    let a = GifBox<int> { payload: 1, held: "one" }
    let b = GifBox<string> { payload: "x", held: "ex" }
    return gif_read(a) == "one" && gif_read(b) == "ex"
}
test "generic_implementor_field_resolves_from_receiver_runtime" {
    assert.is_true(gif_main())
}

// interface_field_via_requires_chain_runtime
interface IfvrNamed { name: string }
interface IfvrAged { age: int }
interface IfvrPerson requires IfvrNamed, IfvrAged {
    occupation: string
}
class IfvrEmployee {
    name: string
    age: int
    occupation: string
    salary: float
    implements IfvrNamed {}
    implements IfvrAged {}
    implements IfvrPerson {}
}
function ifvr_main() -> int {
    let e = IfvrEmployee { name: "Dan", age: 35, occupation: "PM", salary: 1.0 }
    let a: IfvrAged = e
    return a.age
}
test "interface_field_via_requires_chain_runtime" {
    assert.equal(ifvr_main(), 35)
}

// ─── Group M: default.method() ───────────────────────────────────────────────

// default_call_from_override_returns_string
interface DcfoLogger {
    function log(self, msg: string) -> string throws never {
        return "[LOG] " + msg
    }
}
class DcfoTimestampLogger {
    prefix: string
    implements DcfoLogger {
        function log(self, msg: string) -> string {
            return self.prefix + " " + default.log(msg)
        }
    }
}
function dcfo_main() -> string {
    let tl = DcfoTimestampLogger { prefix: "X" }
    return tl.log("test")
}
test "default_call_from_override_returns_string" {
    assert.equal(dcfo_main(), "X [LOG] test")
}

// default_resolves_to_current_block
interface DrtcA {
    function tag(self) -> string throws never { return "A" }
}
interface DrtcB {
    function tag(self) -> string throws never { return "B" }
}
class DrtcX {
    implements DrtcA {
        function tag(self) -> string { return default.tag() + "!" }
    }
    implements DrtcB {}
}
function drtc_main() -> string {
    let x = DrtcX {}
    return x.as<DrtcA>.tag()
}
test "default_resolves_to_current_block" {
    assert.equal(drtc_main(), "A!")
}

// ─── Group N: `.as<I>` projections ───────────────────────────────────────────

// as_projection_same_signature_runtime
interface ApssSerializer {
    function encode(self) -> string throws never
}
interface ApssBinarySerializer {
    function encode(self) -> string throws never
}
class ApssHybrid {
    implements ApssSerializer {
        function encode(self) -> string { return "json:{}" }
    }
    implements ApssBinarySerializer {
        function encode(self) -> string { return "binary" }
    }
}
function apss_main() -> string {
    let h = ApssHybrid {}
    return h.as<ApssSerializer>.encode()
}
test "as_projection_same_signature_runtime" {
    assert.equal(apss_main(), "json:{}")
}

// as_projection_works_when_unambiguous_runtime
interface ApwuAnimal {
    function speak(self) -> string throws never
}
class ApwuDog {
    implements ApwuAnimal {
        function speak(self) -> string { return "Woof!" }
    }
}
function apwu_main() -> string {
    let d = ApwuDog {}
    return d.as<ApwuAnimal>.speak()
}
test "as_projection_works_when_unambiguous_runtime" {
    assert.equal(apwu_main(), "Woof!")
}

// self_as_projection_call_inside_unrelated_block
interface SapcGreeter {
    function greet(self) -> string throws never
}
interface SapcFarewell {
    function bye(self) -> string throws never
}
class SapcPolite {
    name: string
    implements SapcGreeter {
        function greet(self) -> string {
            return "Hello, I'm " + self.name
        }
    }
    implements SapcFarewell {
        function bye(self) -> string {
            return self.as<SapcGreeter>.greet() + " — and goodbye!"
        }
    }
}
function sapc_main() -> string {
    let p = SapcPolite { name: "Alice" }
    return p.bye()
}
test "self_as_projection_call_inside_unrelated_block" {
    assert.equal(sapc_main(), "Hello, I'm Alice — and goodbye!")
}

// diamond_as_projection_call_runtime
interface DapcBase {
    function foo(self) -> string throws never { return "Base" }
}
interface DapcLeft requires DapcBase {
    function foo(self) -> string throws never { return "Left" }
}
interface DapcRight requires DapcBase {
    function foo(self) -> string throws never { return "Right" }
}
class DapcD {
    implements DapcBase {}
    implements DapcLeft {}
    implements DapcRight {}
}
function dapc_main() -> string {
    let d = DapcD {}
    return d.as<DapcLeft>.foo() + ":" + d.as<DapcRight>.foo()
}
test "diamond_as_projection_call_runtime" {
    assert.equal(dapc_main(), "Left:Right")
}

// ─── Group O: dispatch through interface-typed value ─────────────────────────

// interface_typed_var_dispatches_to_concrete
interface ItvdAnimal {
    function speak(self) -> string throws never
}
class ItvdCat {
    implements ItvdAnimal {
        function speak(self) -> string { return "Meow." }
    }
}
function itvd_main() -> string {
    let a: ItvdAnimal = ItvdCat {}
    return a.speak()
}
test "interface_typed_var_dispatches_to_concrete" {
    assert.equal(itvd_main(), "Meow.")
}

// heterogeneous_interface_array_dispatches
interface HiadAnimal {
    function speak(self) -> string throws never
}
class HiadDog {
    implements HiadAnimal {
        function speak(self) -> string { return "Woof!" }
    }
}
class HiadCat {
    implements HiadAnimal {
        function speak(self) -> string { return "Meow." }
    }
}
function hiad_main() -> string {
    let animals: HiadAnimal[] = [HiadDog {}, HiadCat {}]
    return animals[0].speak() + animals[1].speak()
}
test "heterogeneous_interface_array_dispatches" {
    assert.equal(hiad_main(), "Woof!Meow.")
}

// cast_to_parent_interface_via_requires_runtime
interface CtpiNamed { name: string }
interface CtpiPerson requires CtpiNamed {
    function introduce(self) -> string throws never
}
class CtpiEmployee {
    name: string
    implements CtpiNamed {}
    implements CtpiPerson {
        function introduce(self) -> string { return "hi" }
    }
}
function ctpi_main() -> string {
    let e = CtpiEmployee { name: "Alice" }
    let n: CtpiNamed = e
    return n.name
}
test "cast_to_parent_interface_via_requires_runtime" {
    assert.equal(ctpi_main(), "Alice")
}

// default_method_dispatch_through_interface_var
interface DmdtAnimal {
    name: string
    function speak(self) -> string throws never
    function describe(self) -> string throws never {
        return "animal: " + self.name
    }
}
class DmdtDog {
    name: string
    implements DmdtAnimal {
        function speak(self) -> string { return "Woof!" }
    }
}
function dmdt_main() -> string {
    let a: DmdtAnimal = DmdtDog { name: "Rex" }
    return a.describe()
}
test "default_method_dispatch_through_interface_var" {
    assert.equal(dmdt_main(), "animal: Rex")
}

// interface_default_method_calls_self_param_method
interface IdmcEquatable {
    function eq(self, other: Self) -> bool throws never
    function neq(self, other: Self) -> bool throws never {
        return !self.eq(other)
    }
}
class IdmcPair {
    a: int
    implements IdmcEquatable {
        function eq(self, other: Self) -> bool { return self.a == other.a }
    }
}
function idmc_differ<T extends IdmcEquatable>(x: T, y: T) -> bool {
    return x.neq(y)
}
function idmc_main() -> bool {
    return idmc_differ<IdmcPair>(IdmcPair { a: 1 }, IdmcPair { a: 2 })
}
test "interface_default_method_calls_self_param_method" {
    assert.is_true(idmc_main())
}

// generic_bound_self_param_method_call
interface GbspEquatable {
    function eq(self, other: Self) -> bool throws never
}
class GbspPair {
    a: int
    implements GbspEquatable {
        function eq(self, other: Self) -> bool { return self.a == other.a }
    }
}
function gbsp_same<T extends GbspEquatable>(x: T, y: T) -> bool {
    return x.eq(y)
}
function gbsp_main() -> bool {
    return gbsp_same<GbspPair>(GbspPair { a: 7 }, GbspPair { a: 7 })
}
test "generic_bound_self_param_method_call" {
    assert.is_true(gbsp_main())
}

// self_param_method_accepts_matching_nested_self
interface SpmaAdder {
    function addAll(self, others: Self[]) -> int throws never
}
class SpmaAcc {
    base: int
    implements SpmaAdder {
        function addAll(self, others: Self[]) -> int {
            return self.base + others[0].base
        }
    }
}
function spma_combine<T extends SpmaAdder>(x: T, ys: T[]) -> int {
    return x.addAll(ys)
}
function spma_main() -> int {
    return spma_combine<SpmaAcc>(SpmaAcc { base: 10 }, [SpmaAcc { base: 5 }])
}
test "self_param_method_accepts_matching_nested_self" {
    assert.equal(spma_main(), 15)
}

// bound_self_method_value_accepts_matching_arg
interface BsmvAdder {
    function addOne(self, other: Self) -> int throws never
}
class BsmvAcc {
    base: int
    implements BsmvAdder {
        function addOne(self, other: Self) -> int {
            return self.base + other.base
        }
    }
}
function bsmv_combine<T extends BsmvAdder>(x: T, y: T) -> int {
    let f = x.addOne
    return f(y)
}
function bsmv_main() -> int {
    return bsmv_combine<BsmvAcc>(BsmvAcc { base: 10 }, BsmvAcc { base: 5 })
}
test "bound_self_method_value_accepts_matching_arg" {
    assert.equal(bsmv_main(), 15)
}

// interface_default_method_reference_accepts_explicit_receiver
interface IdmrDescribable {
    function describe(self) -> string throws never {
        return "default"
    }
}
class IdmrThing {
    implements IdmrDescribable {}
}
function idmr_main() -> string {
    let t = IdmrThing {}
    let describe = IdmrDescribable.describe
    return describe(t)
}
test "interface_default_method_reference_accepts_explicit_receiver" {
    assert.equal(idmr_main(), "default")
}

// throwing_interface_dispatch_uses_active_catch
class TiduBoom {
    message: string
}
interface TiduFallible {
    function run(self) -> string throws TiduBoom
}
class TiduWorker {
    implements TiduFallible {
        function run(self) -> string throws TiduBoom {
            throw TiduBoom { message: "caught" }
        }
    }
}
function tidu_main() -> string {
    let f: TiduFallible = TiduWorker {}
    return f.run() catch (e) {
        let err: TiduBoom => err.message
    }
}
test "throwing_interface_dispatch_uses_active_catch" {
    assert.equal(tidu_main(), "caught")
}

// override_dispatched_not_default
interface OdndAnimal {
    name: string
    function describe(self) -> string throws never {
        return "default:" + self.name
    }
}
class OdndCat {
    name: string
    implements OdndAnimal {
        function describe(self) -> string {
            return "cat:" + self.name
        }
    }
}
function odnd_main() -> string {
    let a: OdndAnimal = OdndCat { name: "Luna" }
    return a.describe()
}
test "override_dispatched_not_default" {
    assert.equal(odnd_main(), "cat:Luna")
}

// ─── Group P: match narrowing ────────────────────────────────────────────────

// match_narrows_to_concrete_field_access
interface MntcAnimal {
    function speak(self) -> string throws never
}
class MntcDog {
    breed: string
    implements MntcAnimal {
        function speak(self) -> string { return "Woof!" }
    }
}
function mntc_main() -> string {
    let a: MntcAnimal = MntcDog { breed: "Lab" }
    return match (a) {
        let d: MntcDog => d.breed
        _ => "other"
    }
}
test "match_narrows_to_concrete_field_access" {
    assert.equal(mntc_main(), "Lab")
}

// match_destructures_interface_fields
interface MdifAnimal {
    name: string
    function speak(self) -> string throws never
}
class MdifDog {
    name: string
    implements MdifAnimal {
        function speak(self) -> string { return "Woof!" }
    }
}
function mdif_main() -> string {
    let a: MdifAnimal = MdifDog { name: "Rex" }
    return match (a) {
        let d: MdifDog => d.name
        _ => "other"
    }
}
test "match_destructures_interface_fields" {
    assert.equal(mdif_main(), "Rex")
}

// match_destructures_interface_fields_directly
interface MdidAnimal {
    name: string
    function speak(self) -> string throws never
}
class MdidDog {
    name: string
    implements MdidAnimal {
        function speak(self) -> string { return "Woof!" }
    }
}
class MdidCat {
    nickname: string
    implements MdidAnimal {
        name as nickname
        function speak(self) -> string { return "Meow." }
    }
}
function mdid_describe(a: MdidAnimal) -> string {
    return match (a) {
        MdidAnimal { name } => "animal: " + name
    }
}
function mdid_main() -> string {
    let dog: MdidAnimal = MdidDog { name: "Rex" }
    let cat: MdidAnimal = MdidCat { nickname: "Luna" }
    return mdid_describe(dog) + " / " + mdid_describe(cat)
}
test "match_destructures_interface_fields_directly" {
    assert.equal(mdid_main(), "animal: Rex / animal: Luna")
}

// match_interface_destructure_respects_field_subpatterns
interface MidrSwitch {
    active: bool
}
class MidrLamp {
    active: bool
    implements MidrSwitch {}
}
function midr_describe(s: MidrSwitch) -> string {
    return match (s) {
        MidrSwitch { active: true } => "on"
        MidrSwitch { active: false } => "off"
    }
}
function midr_main() -> string {
    let s: MidrSwitch = MidrLamp { active: false }
    return midr_describe(s)
}
test "match_interface_destructure_respects_field_subpatterns" {
    assert.equal(midr_main(), "off")
}

// match_destructures_concrete_implementor_fields
interface MdciAnimal {
    name: string
    function speak(self) -> string throws never
}
class MdciDog {
    name: string
    breed: string
    implements MdciAnimal {
        function speak(self) -> string { return "Woof!" }
    }
}
class MdciCat {
    name: string
    implements MdciAnimal {
        function speak(self) -> string { return "Meow." }
    }
}
function mdci_describe(a: MdciAnimal) -> string {
    return match (a) {
        MdciDog { name, breed } => name + " the " + breed
        MdciCat { name } => name + " the cat"
        _ => "other"
    }
}
function mdci_main() -> string {
    let a: MdciAnimal = MdciDog { name: "Rex", breed: "Lab" }
    return mdci_describe(a)
}
test "match_destructures_concrete_implementor_fields" {
    assert.equal(mdci_main(), "Rex the Lab")
}

// match_open_interface_with_wildcard_works
interface MoiwAnimal {
    function speak(self) -> string throws never
}
class MoiwDog {
    implements MoiwAnimal {
        function speak(self) -> string { return "Woof!" }
    }
}
class MoiwDuck {
    implements MoiwAnimal {
        function speak(self) -> string { return "Quack!" }
    }
}
function moiw_main() -> string {
    let a: MoiwAnimal = MoiwDuck {}
    return match (a) {
        let d: MoiwDog => "dog"
        _ => "other"
    }
}
test "match_open_interface_with_wildcard_works" {
    assert.equal(moiw_main(), "other")
}

// ─── Group Q: generics ───────────────────────────────────────────────────────

// generic_interface_concrete_type_param_runtime
interface GictContainer<T> {
    function add(self, item: T) -> null throws never
    function get(self, index: int) -> T? throws never
    function size(self) -> int throws never
}
class GictIntStack {
    items: int[]
    implements GictContainer<int> {
        function add(self, item: int) -> null {
            self.items.push(item)
            return null
        }
        function get(self, index: int) -> int? {
            return self.items[index]
        }
        function size(self) -> int {
            return self.items.length()
        }
    }
}
function gict_main() -> int {
    let s = GictIntStack { items: [] }
    s.add(10)
    s.add(20)
    return s.size()
}
test "generic_interface_concrete_type_param_runtime" {
    assert.equal(gict_main(), 2)
}

// same_generic_interface_different_type_params_disambiguated_with_as_projection
interface SgidConverter<T> {
    function convert(self) -> T throws never
}
class SgidMultiFormat {
    data: string
    implements SgidConverter<int> {
        function convert(self) -> int { return 42 }
    }
    implements SgidConverter<float> {
        function convert(self) -> float { return 42.5 }
    }
}
function sgid_read_int<T extends SgidConverter<int>>(m: T) -> int {
    return m.as<SgidConverter<int>>.convert()
}
function sgid_read_float<T extends SgidConverter<float>>(m: T) -> float {
    return m.as<SgidConverter<float>>.convert()
}
function sgid_main() -> bool {
    let m = SgidMultiFormat { data: "payload" }
    return sgid_read_int<SgidMultiFormat>(m) == 42
        && sgid_read_float<SgidMultiFormat>(m) == 42.5
}
test "same_generic_interface_different_type_params_disambiguated_with_as_projection" {
    assert.is_true(sgid_main())
}

// generic_bound_runtime
interface GbrNamed { name: string }
class GbrDog {
    name: string
    implements GbrNamed {}
}
function gbr_first_name<T extends GbrNamed>(items: T[]) -> string {
    return items[0].name
}
function gbr_main() -> string {
    let mydog = GbrDog { name: "Rex" }
    mydog.name // works
    let dogs: GbrDog[] = [GbrDog { name: "Rex" }, GbrDog { name: "Buddy" }]
    return gbr_first_name<GbrDog>(dogs)
}
test "generic_bound_runtime" {
    assert.equal(gbr_main(), "Rex")
}

// ─── Group R: reflection ─────────────────────────────────────────────────────

// reflect_type_of_interface_to_string
interface RtitAnimal {
    function speak(self) -> string throws never
}
function rtit_main() -> string {
    return reflect.type_of<RtitAnimal>().to_string()
}
test "reflect_type_of_interface_to_string" {
    assert.equal(rtit_main(), "interfaces.RtitAnimal")
}

// reflect_implements_true_for_implementor
interface RitfAnimal {
    function speak(self) -> string throws never
}
class RitfDog {
    implements RitfAnimal {
        function speak(self) -> string { return "Woof!" }
    }
}
function ritf_main() -> bool {
    return reflect.type_of<RitfDog>().implements(reflect.type_of<RitfAnimal>())
}
test "reflect_implements_true_for_implementor" {
    assert.is_true(ritf_main())
}

// reflect_implements_false_for_non_implementor
interface RiffAnimal {
    function speak(self) -> string throws never
}
class RiffRock {
    mass: int
}
function riff_main() -> bool {
    return reflect.type_of<RiffRock>().implements(reflect.type_of<RiffAnimal>())
}
test "reflect_implements_false_for_non_implementor" {
    assert.is_true(riff_main() == false)
}

// reflect_implemented_by_inverse_runtime
interface RibiAnimal {
    function speak(self) -> string throws never
}
class RibiDog {
    implements RibiAnimal {
        function speak(self) -> string { return "Woof!" }
    }
}
function ribi_main() -> bool {
    let direct = reflect.type_of<RibiDog>().implements(reflect.type_of<RibiAnimal>())
    let reverse = reflect.type_of<RibiAnimal>().implemented_by(reflect.type_of<RibiDog>())
    return direct == reverse
}
test "reflect_implemented_by_inverse_runtime" {
    assert.is_true(ribi_main())
}

// reflect_implements_transitive_via_requires
interface RitvNamed { name: string }
interface RitvPerson requires RitvNamed {
    function introduce(self) -> string throws never
}
class RitvEmployee {
    name: string
    implements RitvNamed {}
    implements RitvPerson {
        function introduce(self) -> string { return "hi" }
    }
}
function ritv_main() -> bool {
    return reflect.type_of<RitvEmployee>().implements(reflect.type_of<RitvNamed>())
}
test "reflect_implements_transitive_via_requires" {
    assert.is_true(ritv_main())
}

// reflect_implements_inside_generic_function
interface RiigAnimal {
    function speak(self) -> string throws never
}
class RiigDog {
    implements RiigAnimal {
        function speak(self) -> string { return "Woof!" }
    }
}
function riig_is_animal<T>() -> bool {
    return reflect.type_of<T>().implements(reflect.type_of<RiigAnimal>())
}
function riig_main() -> bool {
    return riig_is_animal<RiigDog>() && riig_is_animal<int>() == false
}
test "reflect_implements_inside_generic_function" {
    assert.is_true(riig_main())
}

// reflect_interface_does_not_implement_itself
interface RidnAnimal {
    function speak(self) -> string throws never
}
function ridn_main() -> bool {
    return reflect.type_of<RidnAnimal>().implements(reflect.type_of<RidnAnimal>())
}
test "reflect_interface_does_not_implement_itself" {
    assert.is_true(ridn_main() == false)
}

// ─── Group S: class methods + interface coexist ──────────────────────────────

// class_own_method_callable_from_implements_block
interface ComcConfigurable {
    function configure(self) -> string throws never
}
class ComcServer {
    host: string

    function address(self) -> string {
        return self.host
    }

    implements ComcConfigurable {
        function configure(self) -> string {
            return self.address()
        }
    }
}
function comc_main() -> string {
    let s = ComcServer { host: "localhost" }
    return s.configure()
}
test "class_own_method_callable_from_implements_block" {
    assert.equal(comc_main(), "localhost")
}

// class_inherent_method_does_not_override_interface_default_through_existential
interface CimdGreeter {
    function greet(self) -> string throws never {
        return "default"
    }
}
class CimdPerson {
    name: string

    implements CimdGreeter {}

    function greet(self) -> string {
        return self.name
    }
}
function cimd_main() -> string {
    let greeter: CimdGreeter = CimdPerson { name: "Ada" }
    return greeter.greet()
}
test "class_inherent_method_does_not_override_interface_default_through_existential" {
    assert.equal(cimd_main(), "default")
}

// ─── Group U: generic bounds ─────────────────────────────────────────────────

// generic_bound_extends_named_runtime
interface GbenNamed { name: string }
class GbenDog {
    name: string
    implements GbenNamed {}
}
function gben_first_name<T extends GbenNamed>(items: T[]) -> string {
    return items[0].name
}
function gben_main() -> string {
    let dogs: GbenDog[] = [GbenDog { name: "Rex" }, GbenDog { name: "Buddy" }]
    return gben_first_name<GbenDog>(dogs)
}
test "generic_bound_extends_named_runtime" {
    assert.equal(gben_main(), "Rex")
}

// generic_bound_as_projection_selects_generic_interface_instantiation
interface GbapConverter<T> {
    function convert(self) -> T throws never
}
class GbapMultiFormat {
    data: string
    implements GbapConverter<int> {
        function convert(self) -> int { return 42 }
    }
    implements GbapConverter<float> {
        function convert(self) -> float { return 42.5 }
    }
}
function gbap_read_int<T extends GbapConverter<int>>(m: T) -> int {
    return m.as<GbapConverter<int>>.convert()
}
function gbap_main() -> int {
    return gbap_read_int<GbapMultiFormat>(GbapMultiFormat { data: "payload" })
}
test "generic_bound_as_projection_selects_generic_interface_instantiation" {
    assert.equal(gbap_main(), 42)
}

// generic_bound_direct_method_call_dispatches_through_interface
interface GbdmConverter<T> {
    function convert(self) -> T throws never
}
class GbdmIntBox {
    value: int
    implements GbdmConverter<int> {
        function convert(self) -> int { return self.value }
    }
}
function gbdm_read_int<T extends GbdmConverter<int>>(m: T) -> int {
    return m.convert()
}
function gbdm_main() -> int {
    return gbdm_read_int<GbdmIntBox>(GbdmIntBox { value: 42 })
}
test "generic_bound_direct_method_call_dispatches_through_interface" {
    assert.equal(gbdm_main(), 42)
}

// generic_interface_method_preserves_method_type_param
interface GimpEcho<T> {
    function echo<U>(self, value: U) -> U throws never
}
class GimpEchoer {
    implements GimpEcho<int> {
        function echo<U>(self, value: U) -> U {
            return value
        }
    }
}
function gimp_main() -> string {
    let e: GimpEcho<int> = GimpEchoer {}
    return e.echo<string>("ok")
}
test "generic_interface_method_preserves_method_type_param" {
    assert.equal(gimp_main(), "ok")
}

// generic_bound_through_extends_chain
interface GbteNamed { name: string }
interface GbtePerson requires GbteNamed {
    occupation: string
}
class GbteEmployee {
    name: string
    occupation: string
    implements GbteNamed {}
    implements GbtePerson {}
}
function gbte_greet<T extends GbtePerson>(p: T) -> string {
    return "Hi, " + p.name + " the " + p.occupation
}
function gbte_main() -> string {
    let e = GbteEmployee { name: "Alice", occupation: "PM" }
    return gbte_greet<GbteEmployee>(e)
}
test "generic_bound_through_extends_chain" {
    assert.equal(gbte_main(), "Hi, Alice the PM")
}

// ─── Group Y: Self return / param types ──────────────────────────────────────

// self_return_type_carries_concrete_class
interface SrtcCloneable {
    function clone(self) -> Self throws never
}
class SrtcBox {
    value: int
    implements SrtcCloneable {
        function clone(self) -> Self {
            return SrtcBox { value: self.value }
        }
    }
}
function srtc_main() -> int {
    let b = SrtcBox { value: 42 }
    let c = b.clone()
    return c.value
}
test "self_return_type_carries_concrete_class" {
    assert.equal(srtc_main(), 42)
}

// self_return_on_interface_typed_receiver_collapses_to_interface
interface SritCloneable {
    value: int
    function clone(self) -> Self throws never
}
class SritBox {
    value: int
    implements SritCloneable {
        function clone(self) -> Self {
            return SritBox { value: self.value }
        }
    }
}
function srit_main() -> int {
    let c: SritCloneable = SritBox { value: 42 }
    let cloned = c.clone()
    return cloned.value
}
test "self_return_on_interface_typed_receiver_collapses_to_interface" {
    assert.equal(srit_main(), 42)
}

// multi_self_method_accepts_concrete_receiver
interface MsmaEquatable {
    function same(self, other: Self) -> bool throws never
}
class MsmaBox {
    value: int
    implements MsmaEquatable {
        function same(self, other: Self) -> bool {
            return self.value == other.value
        }
    }
}
function msma_main() -> bool {
    let a = MsmaBox { value: 7 }
    let b = MsmaBox { value: 7 }
    return a.same(b)
}
test "multi_self_method_accepts_concrete_receiver" {
    assert.is_true(msma_main())
}

// concrete_receiver_inherited_default_self_param_method
interface CridEquals {
    function eq(self, other: Self) -> bool throws never
    function neq(self, other: Self) -> bool  throws never {
        return !self.eq(other)
    }
}
class CridNum {
    v: int
    implements CridEquals {
        function eq(self, other: Self) -> bool { return self.v == other.v }
    }
}
function crid_main() -> bool {
    return CridNum { v: 1 }.neq(CridNum { v: 2 })
}
test "concrete_receiver_inherited_default_self_param_method" {
    assert.is_true(crid_main())
}

// bounded_generic_forwarded_to_bounded_call_is_accepted
interface BgfbEquatable {
    function eq(self, other: Self) -> bool throws never
}
class BgfbPair {
    a: int
    implements BgfbEquatable {
        function eq(self, other: Self) -> bool { return self.a == other.a }
    }
}
function bgfb_same<T extends BgfbEquatable>(x: T) -> bool {
    return x.eq(x)
}
function bgfb_forward<V extends BgfbEquatable>(x: V) -> bool {
    return bgfb_same(x)
}
function bgfb_main() -> bool {
    return bgfb_forward<BgfbPair>(BgfbPair { a: 1 })
}
test "bounded_generic_forwarded_to_bounded_call_is_accepted" {
    assert.is_true(bgfb_main())
}

// ─── Group Z: interface-to-interface casting ─────────────────────────────────

// interface_to_interface_conversion_via_unknown_match_narrowing
interface IticAnimal {
    function speak(self) -> string throws never
}
interface IticSwimmer {
    function swim(self) -> string throws never
}
class IticDog {
    implements IticAnimal {
        function speak(self) -> string { return "Woof!" }
    }
    implements IticSwimmer {
        function swim(self) -> string { return "splash" }
    }
}
function itic_main() -> string {
    let a: IticAnimal = IticDog {}
    let value: unknown = a
    return match (value) {
        let d: IticDog => {
            let s: IticSwimmer = d
            s.swim()
        }
        _ => "not a swimmer"
    }
}
test "interface_to_interface_conversion_via_unknown_match_narrowing" {
    assert.equal(itic_main(), "splash")
}

// as_upcast_selects_interface_field_link_runtime
interface AusiNamed { name: string }
interface AusiLabeled { name: string }
class AusiItem {
    named_name: string
    labeled_name: string
    implements AusiNamed { name as named_name }
    implements AusiLabeled { name as labeled_name }
}
function ausi_main() -> bool {
    let i = AusiItem { named_name: "widget", labeled_name: "WIDGET-001" }
    return i.as<AusiNamed>.name == "widget"
        && i.as<AusiLabeled>.name == "WIDGET-001"
}
test "as_upcast_selects_interface_field_link_runtime" {
    assert.is_true(ausi_main())
}

// ─── Group AA: dispatch edge cases — non-local receivers ─────────────────────

// dispatch_through_function_call_result
interface DtfcAnimal {
    function speak(self) -> string throws never
}
class DtfcCat {
    implements DtfcAnimal {
        function speak(self) -> string { return "Meow." }
    }
}
function dtfc_get_animal() -> DtfcAnimal {
    return DtfcCat {}
}
function dtfc_main() -> string {
    return dtfc_get_animal().speak()
}
test "dispatch_through_function_call_result" {
    assert.equal(dtfc_main(), "Meow.")
}

// dispatch_through_array_index_with_field_access
interface DtaiAnimal {
    name: string
    function speak(self) -> string throws never
}
class DtaiCat {
    name: string
    implements DtaiAnimal {
        function speak(self) -> string { return "Meow." }
    }
}
function dtai_main() -> string {
    let animals: DtaiAnimal[] = [DtaiCat { name: "Luna" }]
    return animals[0].name
}
test "dispatch_through_array_index_with_field_access" {
    assert.equal(dtai_main(), "Luna")
}

// dispatch_through_field_access_chain
interface DtfaAnimal {
    function speak(self) -> string throws never
}
class DtfaCat {
    implements DtfaAnimal {
        function speak(self) -> string { return "Meow." }
    }
}
class DtfaWrapper {
    animal: DtfaAnimal
}
function dtfa_main() -> string {
    let w = DtfaWrapper { animal: DtfaCat {} }
    return w.animal.speak()
}
test "dispatch_through_field_access_chain" {
    assert.equal(dtfa_main(), "Meow.")
}

// ─── Group AB: diamond + multi-level requires ────────────────────────────────

// requires_chain_four_levels_deep
interface RcflA { function tag(self) -> string throws never }
interface RcflB requires RcflA {}
interface RcflC requires RcflB {}
interface RcflD requires RcflC {}
class RcflLeaf {
    implements RcflA { function tag(self) -> string { return "leaf" } }
    implements RcflB {}
    implements RcflC {}
    implements RcflD {}
}
function rcfl_main() -> string {
    let l: RcflA = RcflLeaf {}
    return l.tag()
}
test "requires_chain_four_levels_deep" {
    assert.equal(rcfl_main(), "leaf")
}

// diamond_call_through_interface_typed_var
interface DctiBase { function foo(self) -> string  throws never { return "Base" } }
interface DctiLeft requires DctiBase { function foo(self) -> string  throws never { return "Left" } }
interface DctiRight requires DctiBase { function foo(self) -> string  throws never { return "Right" } }
class DctiD {
    implements DctiBase {}
    implements DctiLeft {}
    implements DctiRight {}
}
function dcti_main() -> string {
    let l: DctiLeft = DctiD {}
    let r: DctiRight = DctiD {}
    return l.foo() + ":" + r.foo()
}
test "diamond_call_through_interface_typed_var" {
    assert.equal(dcti_main(), "Left:Right")
}

// ─── Group AD: dispatch through chained method calls ─────────────────────────

// dispatch_chained_three_function_calls_returning_interfaces
interface DctfAnimal {
    function speak(self) -> string throws never
}
class DctfCat {
    implements DctfAnimal {
        function speak(self) -> string { return "Meow." }
    }
}
function dctf_fresh() -> DctfAnimal {
    return DctfCat {}
}
function dctf_rebox(a: DctfAnimal) -> DctfAnimal {
    return a
}
function dctf_main() -> string {
    return dctf_rebox(dctf_rebox(dctf_fresh())).speak()
}
test "dispatch_chained_three_function_calls_returning_interfaces" {
    assert.equal(dctf_main(), "Meow.")
}

// dispatch_chained_methods_returning_interface_each_level
interface DcmrAnimal {
    function speak(self) -> string throws never
    function next(self) -> DcmrAnimal throws never { return self }
}
class DcmrCat {
    implements DcmrAnimal {
        function speak(self) -> string { return "Meow." }
    }
}
function dcmr_main() -> string {
    let c = DcmrCat {}
    let a: DcmrAnimal = c
    return a.next().next().next().speak()
}
test "dispatch_chained_methods_returning_interface_each_level" {
    assert.equal(dcmr_main(), "Meow.")
}

// ─── Group AE: dispatch through deeply nested field/array/index chains ───────

// dispatch_through_nested_array_of_arrays
interface DtnaAnimal {
    function speak(self) -> string throws never
}
class DtnaDog {
    implements DtnaAnimal {
        function speak(self) -> string { return "Woof!" }
    }
}
class DtnaCat {
    implements DtnaAnimal {
        function speak(self) -> string { return "Meow." }
    }
}
function dtna_main() -> string {
    let pack: DtnaAnimal[][] = [
        [DtnaDog {}, DtnaCat {}],
        [DtnaCat {}, DtnaDog {}],
    ]
    return pack[0][1].speak() + ":" + pack[1][0].speak()
}
test "dispatch_through_nested_array_of_arrays" {
    assert.equal(dtna_main(), "Meow.:Meow.")
}

// dispatch_through_field_of_array_of_interface
interface DtfoAnimal {
    function speak(self) -> string throws never
}
class DtfoDog {
    implements DtfoAnimal {
        function speak(self) -> string { return "Woof!" }
    }
}
class DtfoCat {
    implements DtfoAnimal {
        function speak(self) -> string { return "Meow." }
    }
}
class DtfoWrapper {
    pets: DtfoAnimal[]
}
function dtfo_main() -> string {
    let w = DtfoWrapper { pets: [DtfoDog {}, DtfoCat {}] }
    return w.pets[1].speak()
}
test "dispatch_through_field_of_array_of_interface" {
    assert.equal(dtfo_main(), "Meow.")
}

// dispatch_through_five_level_field_chain
interface DtflAnimal {
    function speak(self) -> string throws never
}
class DtflCat {
    implements DtflAnimal {
        function speak(self) -> string { return "Meow." }
    }
}
class DtflL1 { e: DtflAnimal }
class DtflL2 { d: DtflL1 }
class DtflL3 { c: DtflL2 }
class DtflL4 { b: DtflL3 }
class DtflL5 { a: DtflL4 }
function dtfl_main() -> string {
    let r = DtflL5 { a: DtflL4 { b: DtflL3 { c: DtflL2 { d: DtflL1 { e: DtflCat {} } } } } }
    return r.a.b.c.d.e.speak()
}
test "dispatch_through_five_level_field_chain" {
    assert.equal(dtfl_main(), "Meow.")
}

// dispatch_through_map_value_field_then_method
interface DtmvAnimal {
    function speak(self) -> string throws never
}
class DtmvCat {
    implements DtmvAnimal {
        function speak(self) -> string { return "Meow." }
    }
}
function dtmv_main() -> string {
    let directory: map<string, DtmvAnimal> = { "luna": DtmvCat {} }
    return directory["luna"].speak()
}
test "dispatch_through_map_value_field_then_method" {
    assert.equal(dtmv_main(), "Meow.")
}

// ─── Group AF: dispatch combined with control flow ───────────────────────────

// dispatch_through_branch_assigned_interface
interface DtbaAnimal {
    function speak(self) -> string throws never
}
class DtbaDog {
    implements DtbaAnimal {
        function speak(self) -> string { return "Woof!" }
    }
}
class DtbaCat {
    implements DtbaAnimal {
        function speak(self) -> string { return "Meow." }
    }
}
function dtba_pick(use_dog: bool) -> DtbaAnimal {
    if (use_dog) {
        return DtbaDog {}
    } else {
        return DtbaCat {}
    }
}
function dtba_main() -> string {
    return dtba_pick(true).speak() + ":" + dtba_pick(false).speak()
}
test "dispatch_through_branch_assigned_interface" {
    assert.equal(dtba_main(), "Woof!:Meow.")
}

// dispatch_for_loop_over_interface_array_collects_results
interface DfloAnimal {
    function speak(self) -> string throws never
}
class DfloDog {
    implements DfloAnimal {
        function speak(self) -> string { return "Woof!" }
    }
}
class DfloCat {
    implements DfloAnimal {
        function speak(self) -> string { return "Meow." }
    }
}
class DfloDuck {
    implements DfloAnimal {
        function speak(self) -> string { return "Quack!" }
    }
}
function dflo_main() -> string {
    let zoo: DfloAnimal[] = [DfloDog {}, DfloCat {}, DfloDuck {}, DfloCat {}, DfloDog {}]
    let acc: string = ""
    for (let a in zoo) {
        acc = acc + a.speak() + ","
    }
    return acc
}
test "dispatch_for_loop_over_interface_array_collects_results" {
    assert.equal(dflo_main(), "Woof!,Meow.,Quack!,Meow.,Woof!,")
}

// ─── Group AG: diamond / requires — deeper trees ─────────────────────────────

// diamond_six_level_requires_chain
interface DslrA { function tag(self) -> string throws never }
interface DslrB requires DslrA {}
interface DslrC requires DslrB {}
interface DslrD requires DslrC {}
interface DslrE requires DslrD {}
interface DslrF requires DslrE {}
class DslrDeep {
    implements DslrA { function tag(self) -> string { return "deep" } }
    implements DslrB {}
    implements DslrC {}
    implements DslrD {}
    implements DslrE {}
    implements DslrF {}
}
function dslr_main() -> string {
    let a: DslrA = DslrDeep {}
    let b: DslrB = DslrDeep {}
    let c: DslrC = DslrDeep {}
    let d: DslrD = DslrDeep {}
    let e: DslrE = DslrDeep {}
    let f: DslrF = DslrDeep {}
    return a.tag() + b.tag() + c.tag() + d.tag() + e.tag() + f.tag()
}
test "diamond_six_level_requires_chain" {
    assert.equal(dslr_main(), "deepdeepdeepdeepdeepdeep")
}

// diamond_with_overrides_at_each_level
interface DwoaBase { function name(self) -> string  throws never { return "Base" } }
interface DwoaMid requires DwoaBase { function name(self) -> string  throws never { return "Mid" } }
interface DwoaTip requires DwoaMid { function name(self) -> string  throws never { return "Tip" } }
class DwoaConcrete {
    implements DwoaBase {}
    implements DwoaMid {}
    implements DwoaTip {}
}
function dwoa_main() -> string {
    let b: DwoaBase = DwoaConcrete {}
    let m: DwoaMid = DwoaConcrete {}
    let t: DwoaTip = DwoaConcrete {}
    return b.name() + ":" + m.name() + ":" + t.name()
}
test "diamond_with_overrides_at_each_level" {
    assert.equal(dwoa_main(), "Base:Mid:Tip")
}

// double_diamond_two_independent_inheritance_paths
interface DdtiBaseTag {}
interface DdtiLeft requires DdtiBaseTag { function left_tag(self) -> string throws never }
interface DdtiRight requires DdtiBaseTag { function right_tag(self) -> string throws never }
interface DdtiBaseNote {}
interface DdtiX requires DdtiBaseNote { function x_note(self) -> string throws never }
interface DdtiY requires DdtiBaseNote { function y_note(self) -> string throws never }
class DdtiHub {
    implements DdtiBaseTag {}
    implements DdtiLeft {
        function left_tag(self) -> string { return "L" }
    }
    implements DdtiRight {
        function right_tag(self) -> string { return "R" }
    }
    implements DdtiBaseNote {}
    implements DdtiX {
        function x_note(self) -> string { return "X" }
    }
    implements DdtiY {
        function y_note(self) -> string { return "Y" }
    }
}
function ddti_main() -> string {
    let h = DdtiHub {}
    return h.left_tag() + h.right_tag() + h.x_note() + h.y_note()
}
test "double_diamond_two_independent_inheritance_paths" {
    assert.equal(ddti_main(), "LRXY")
}

// ─── Group AH: `default` keyword corner cases ────────────────────────────────

// default_called_twice_in_one_override_body
interface DctioLogger {
    function log(self, msg: string) -> string throws never {
        return "[L] " + msg
    }
}
class DctioDoubleLogger {
    prefix: string
    implements DctioLogger {
        function log(self, msg: string) -> string {
            return self.prefix + default.log(msg) + " " + default.log("END")
        }
    }
}
function dctio_main() -> string {
    let dl = DctioDoubleLogger { prefix: "PRE-" }
    return dl.log("hi")
}
test "default_called_twice_in_one_override_body" {
    assert.equal(dctio_main(), "PRE-[L] hi [L] END")
}

// default_call_with_computed_argument_expression
interface DcwcWrapper {
    function decorate(self, body: string) -> string throws never {
        return "[" + body + "]"
    }
}
class DcwcStamp {
    mark: string
    implements DcwcWrapper {
        function decorate(self, body: string) -> string {
            return default.decorate(self.mark + ":" + body)
        }
    }
}
function dcwc_main() -> string {
    let s = DcwcStamp { mark: "OK" }
    return s.decorate("hi")
}
test "default_call_with_computed_argument_expression" {
    assert.equal(dcwc_main(), "[OK:hi]")
}

// default_keyword_shadowed_by_local_variable_resolves_to_local
interface DkslLogger {
    function log(self, msg: string) -> string throws never { return msg }
}
class DkslL {
    implements DkslLogger {
        function log(self, msg: string) -> string {
            let default: string = "shadow"
            return default
        }
    }
}
function dksl_main() -> string {
    let l = DkslL {}
    return l.log("ignored")
}
test "default_keyword_shadowed_by_local_variable_resolves_to_local" {
    assert.equal(dksl_main(), "shadow")
}

// ─── Group AI: explicit requires satisfaction ────────────────────────────────

// requires_chain_exposes_parent_fields
interface RcepNamed { name: string }
interface RcepPerson requires RcepNamed {
    occupation: string
}
class RcepEmployee {
    name: string
    occupation: string
    implements RcepNamed {}
    implements RcepPerson {}
}
function rcep_main() -> string {
    let p: RcepPerson = RcepEmployee { name: "Alice", occupation: "PM" }
    return p.name
}
test "requires_chain_exposes_parent_fields" {
    assert.equal(rcep_main(), "Alice")
}

// requires_chain_parent_field_in_default_method
interface RcpfNamed { name: string }
interface RcpfPerson requires RcpfNamed {
    occupation: string
    function introduce(self) -> string throws never {
        return self.name + " the " + self.occupation
    }
}
class RcpfEmployee {
    name: string
    occupation: string
    implements RcpfNamed {}
    implements RcpfPerson {}
}
function rcpf_main() -> string {
    let p: RcpfPerson = RcpfEmployee { name: "Alice", occupation: "PM" }
    return p.introduce()
}
test "requires_chain_parent_field_in_default_method" {
    assert.equal(rcpf_main(), "Alice the PM")
}

// ─── Group AJ: out-of-body implements (`implements I for T`) ─────────────────

// out_of_body_implements_is_visible_to_reflection_registry
interface OobvAnimal {
    function speak(self) -> string throws never
}
class OobvDog {}
implements OobvAnimal for OobvDog {
    function speak(self) -> string { return "woof" }
}
function oobv_main() -> bool {
    return reflect.type_of<OobvDog>().implements(reflect.type_of<OobvAnimal>())
        && reflect.type_of<OobvAnimal>().implemented_by(reflect.type_of<OobvDog>())
}
test "out_of_body_implements_is_visible_to_reflection_registry" {
    assert.is_true(oobv_main())
}

// out_of_body_implements_for_primitive_as_projection_runtime
// (Primitive method name is debugOobp to avoid ambiguity in shared namespace.)
interface OobpDebuggable {
    function debugOobp(self) -> string throws never
}
implements OobpDebuggable for int {
    function debugOobp(self) -> string { return "int" }
}
function oobp_main() -> string {
    let myInteger: int = 1
    let asDebuggable: OobpDebuggable = myInteger
    return myInteger.as<OobpDebuggable>.debugOobp() + ":" + asDebuggable.debugOobp()
}
test "out_of_body_implements_for_primitive_as_projection_runtime" {
    assert.equal(oobp_main(), "int:int")
}

// default_call_from_out_of_body_override_runtime
interface DcobLogger {
    function log(self, msg: string) -> string throws never {
        return "[L] " + msg
    }
}
class DcobPrefixLogger {
    prefix: string
}
implements DcobLogger for DcobPrefixLogger {
    function log(self, msg: string) -> string {
        return self.prefix + default.log(msg)
    }
}
function dcob_main() -> string {
    let logger: DcobLogger = DcobPrefixLogger { prefix: "P:" }
    return logger.log("hi")
}
test "default_call_from_out_of_body_override_runtime" {
    assert.equal(dcob_main(), "P:[L] hi")
}

// default_call_from_generic_out_of_body_override_runtime
interface DcgoLogger {
    function log(self, msg: string) -> string throws never {
        return "[L] " + msg
    }
}
class DcgoBox<T> {
    prefix: string
    value: T
}
implements<T> DcgoLogger for DcgoBox<T> {
    function log(self, msg: string) -> string {
        return self.prefix + default.log(msg)
    }
}
function dcgo_main() -> string {
    let logger: DcgoLogger = DcgoBox<int> { prefix: "P:", value: 1 }
    return logger.log("hi")
}
test "default_call_from_generic_out_of_body_override_runtime" {
    assert.equal(dcgo_main(), "P:[L] hi")
}

// generic_out_of_body_override_preserves_method_type_args_runtime
interface GoopDescriber<T> {
    function describe<U>(self, value: U) -> string throws never
}
class GoopBox<T> {
    value: T
}
implements<T> GoopDescriber<T> for GoopBox<T> {
    function describe<U>(self, value: U) -> string {
        return reflect.type_of<T>().to_string() + ":" + reflect.type_of<U>().to_string()
    }
}
function goop_main() -> string {
    let d: GoopDescriber<int> = GoopBox<int> { value: 1 }
    return d.describe<string>("ok")
}
test "generic_out_of_body_override_preserves_method_type_args_runtime" {
    assert.equal(goop_main(), "int:string")
}

// generic_requires_parent_args_dispatch_on_class_implementor
interface GrpaParent<T> {
    function describe(self) -> string throws never
}
interface GrpaChild<T> requires GrpaParent<T> {}
class GrpaBox {
    implements GrpaParent<int> {
        function describe(self) -> string { return "parent-int" }
    }
    implements GrpaChild<int> {}
}
function grpa_main() -> string {
    let child: GrpaChild<int> = GrpaBox {}
    return child.describe()
}
test "generic_requires_parent_args_dispatch_on_class_implementor" {
    assert.equal(grpa_main(), "parent-int")
}

// generic_requires_parent_field_args_runtime
interface GrpfParent<T> {
    value: T
}
interface GrpfChild<T> requires GrpfParent<T> {}
class GrpfBox {
    value: int
    implements GrpfParent<int> {}
    implements GrpfChild<int> {}
}
function grpf_main() -> int {
    let child: GrpfChild<int> = GrpfBox { value: 42 }
    return child.value
}
test "generic_requires_parent_field_args_runtime" {
    assert.equal(grpf_main(), 42)
}

// generic_requires_parent_field_alias_runtime
interface GrplParent<T> {
    value: T
}
interface GrplChild<T> requires GrplParent<T> {}
class GrplBox {
    stored: int
    implements GrplParent<int> {
        value as stored
    }
    implements GrplChild<int> {}
}
function grpl_main() -> int {
    let child: GrplChild<int> = GrplBox { stored: 42 }
    return child.value
}
test "generic_requires_parent_field_alias_runtime" {
    assert.equal(grpl_main(), 42)
}

// generic_requires_parent_args_dispatch_on_type_implementor
// (Primitive method name is describeGrpt to avoid ambiguity in shared namespace.)
interface GrptParent<T> {
    function describeGrpt(self) -> string throws never
}
interface GrptChild<T> requires GrptParent<T> {}
implements GrptParent<int> for int {
    function describeGrpt(self) -> string { return "parent-int" }
}
implements GrptChild<int> for int {}
function grpt_main() -> string {
    let child: GrptChild<int> = 1
    return child.describeGrpt()
}
test "generic_requires_parent_args_dispatch_on_type_implementor" {
    assert.equal(grpt_main(), "parent-int")
}

// requires_closure_preserves_multiple_parent_instantiations_runtime
interface RcpmParent<T> {
    function describe(self) -> string throws never
}
interface RcpmNeedsInt requires RcpmParent<int> {}
interface RcpmNeedsString requires RcpmParent<string> {}
interface RcpmBoth requires RcpmNeedsInt, RcpmNeedsString {}
class RcpmBox {
    implements RcpmParent<int> {
        function describe(self) -> string { return "int" }
    }
    implements RcpmParent<string> {
        function describe(self) -> string { return "string" }
    }
    implements RcpmNeedsInt {}
    implements RcpmNeedsString {}
    implements RcpmBoth {}
}
function rcpm_main() -> bool {
    let both: RcpmBoth = RcpmBox {}
    return both.as<RcpmParent<int>>.describe() == "int"
        && both.as<RcpmParent<string>>.describe() == "string"
}
test "requires_closure_preserves_multiple_parent_instantiations_runtime" {
    assert.is_true(rcpm_main())
}

// interface_type_args_match_positionally_runtime
// Replaces a compile-time unit test that pinned the same rule on the deleted
// block-selection path: interface type arguments match *by position*, so
// `Conv<int, string>` and `Conv<string, int>` are different instantiations. One
// class implements both, so the receiver cannot discriminate them — only the
// requested view's argument order can. Order-insensitive matching would resolve
// both views to whichever rule the registry happens to hold first.
interface ItampConv<A, B> {
    function tag(self) -> string throws never
}
class ItampBoth {
    implements ItampConv<int, string> {
        function tag(self) -> string throws never { "int->string" }
    }
    implements ItampConv<string, int> {
        function tag(self) -> string throws never { "string->int" }
    }
}
function itamp_main() -> bool {
    let p = ItampBoth {}
    let forward: ItampConv<int, string> = p
    let reverse: ItampConv<string, int> = p
    return forward.tag() == "int->string" && reverse.tag() == "string->int"
}
test "interface_type_args_match_positionally_runtime" {
    assert.is_true(itamp_main())
}
