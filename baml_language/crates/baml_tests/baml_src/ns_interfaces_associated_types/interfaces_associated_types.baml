// Runtime behavior of BEP-057 associated types on interfaces.
//
// Workaround for the test-block local-boxing VM bug: helpers are used
// instead of block-local variables. Type and function names are alpha-renamed
// with per-test prefixes (e.g. `Source` -> `RmfSource`) to avoid namespace
// collisions within the shared compile.

// ─── associated_type_projection_in_throws_position_runs ──────────────────────

class Boom {
    message: string
}

interface Fallible {
    type Error

    function value(self) -> int throws Self.Error

    function value_plus_one(self) -> int throws Self.Error {
        return self.value() + 1
    }
}

class AlwaysFails {
    implements Fallible {
        type Error = Boom

        function value(self) -> int throws Boom {
            throw Boom { message: "boom" }
        }
    }
}

function call_value<F extends Fallible>(value: F) -> int throws F.Error {
    return value.value()
}

function atp_throws_main() -> string {
    let default_result = AlwaysFails {}.value_plus_one() catch (e) {
        Boom => "default:" + e.message
    }
    let generic_result = call_value(AlwaysFails {}) catch (e) {
        Boom => "generic:" + e.message
    }
    // The catch arms make these `int | string` (the int is the
    // non-throwing value); render explicitly — a mixed union is not
    // a valid concat operand.
    return default_result.to_string() + "|" + generic_result.to_string()
}

test "associated_type_projection_in_throws_position_runs" {
    assert.equal(atp_throws_main(), "default:boom|generic:boom")
}

class CaeKaboom {
    data: int
}

interface CaeDriver<Input> {
    type Output
    type Error

    function drive(self, input: Input) -> Self.Output throws Self.Error
}

class CaeTask<T> {
    value: T

    function drive<Output, Error, D extends CaeDriver<CaeTask<T>, Output = Output, Error = Error>>(
        self,
        driver: D,
    ) -> Output throws Error {
        driver.drive(self)
    }
}

class CaeIntDriver {}

implements CaeDriver<CaeTask<int>> for CaeIntDriver {
    type Output = int
    type Error = CaeKaboom

    function drive(self, input: CaeTask<int>) -> int throws CaeKaboom {
        throw CaeKaboom { data: input.value }
    }
}

function cae_main() -> int {
    CaeTask<int> { value: 42 }.drive(CaeIntDriver {}) catch (e) {
        let error: CaeKaboom => error.data
    }
}

test "concrete_associated_error_binds_generic_throws_at_call_site" {
    assert.equal(cae_main(), 42)
}

// ─── default_method_self_call_yielding_associated_type_runs ──────────────────

interface Describable {
    type Output

    function value(self) -> Self.Output throws never

    function describe(self) -> Self.Output throws never {
        return self.value()
    }
}

class Ticket {
    id: string

    implements Describable {
        type Output = string

        function value(self) -> Self.Output {
            return self.id
        }
    }
}

function dms_main() -> string {
    let ticket = Ticket { id: "A-100" }
    return ticket.describe()
}

test "default_method_self_call_yielding_associated_type_runs" {
    assert.equal(dms_main(), "A-100")
}

// ─── blanket_impl_self_associated_projection_uses_bounded_typevar ────────────

interface BspSource {
    type Item
    name: string
    function get(self) -> Self.Item throws never
}

class BspTextSource {
    name: string
    text: string

    implements BspSource {
        type Item = string

        function get(self) -> Self.Item {
            return self.text
        }
    }
}

interface BspRenderable {
    type Output
    function render(self) -> Self.Output throws never
}

class BspWrapped<T extends BspSource<Item = string>> {
    inner: T
}

implements<T extends BspSource<Item = string>> BspRenderable for BspWrapped<T> {
    type Output = T.Item

    function render(self) -> Self.Output {
        return self.inner.get()
    }
}

function bsp_take_source<T extends BspSource<Item = string>>(source: T) -> T.Item {
    return source.get()
}

function bsp_main() -> string {
    let source = BspTextSource { name: "sample", text: "ok" }
    let wrapped = BspWrapped<BspTextSource> { inner: source }

    return bsp_take_source(source) + ":" + wrapped.as<BspRenderable<Output = string>>.render()
}

test "blanket_impl_self_associated_projection_uses_bounded_typevar" {
    assert.equal(bsp_main(), "ok:ok")
}

// ─── blanket_impl_concrete_projection_return_resolves_at_callsite ────────────

interface BcpSource {
    type Item
    function get(self) -> Self.Item throws never
}

class BcpTextSource {
    text: string

    implements BcpSource {
        type Item = string

        function get(self) -> Self.Item {
            return self.text
        }
    }
}

interface BcpWrapperView {
    type Output
    function output(self) -> Self.Output throws never
}

class BcpWrapped<S extends BcpSource<Item = string>> {
    inner: S
}

implements<S extends BcpSource<Item = string>> BcpWrapperView for BcpWrapped<S> {
    type Output = S.Item

    function output(self) -> Self.Output {
        return self.inner.get()
    }
}

function bcp_main() -> string {
    let wrapped = BcpWrapped<BcpTextSource> { inner: BcpTextSource { text: "hello" } }
    return wrapped.output()
}

test "blanket_impl_concrete_projection_return_resolves_at_callsite" {
    assert.equal(bcp_main(), "hello")
}

// ─── upcast_of_bounded_typevar_preserves_associated_bindings ─────────────────

interface UpcHasKey {
    type Key
    key: Self.Key
}

interface UpcNamed {
    name: string
}

interface UpcEntity requires UpcHasKey<Key = string>, UpcNamed {
}

class UpcUser {
    id: string
    name: string

    implements UpcHasKey {
        type Key = string
        key as id
    }

    implements UpcNamed {
    }

    implements UpcEntity {
    }
}

class UpcEntityBox<T extends UpcEntity> {
    value: T
}

interface UpcSummarizes {
    type Key
    function key(self) -> Self.Key throws never
    function summary(self) -> string throws never
}

implements<T extends UpcEntity> UpcSummarizes for UpcEntityBox<T> {
    type Key = (T as UpcHasKey).Key

    function key(self) -> Self.Key {
        return self.value.as<UpcHasKey<Key = string>>.key
    }

    function summary(self) -> string {
        return self.value.name + ":" + self.key()
    }
}

function upc_entity_key<T extends UpcEntity>(value: T) -> (T as UpcHasKey).Key {
    return value.as<UpcHasKey<Key = string>>.key
}

function upc_main() -> string {
    let user = UpcUser { id: "u1", name: "Ada" }
    let boxed = UpcEntityBox<UpcUser> { value: user }
    return boxed.summary() + "|" + upc_entity_key(user)
}

test "upcast_of_bounded_typevar_preserves_associated_bindings" {
    assert.equal(upc_main(), "Ada:u1|u1")
}

// ─── associated_type_default_typevar_satisfies_declared_bound ────────────────

interface AdbSummarizable {
    function summary(self) -> string throws never
}

interface AdbHolder<T extends AdbSummarizable> {
    type Item extends AdbSummarizable = T

    function get(self) -> Self.Item throws never
}

class AdbUser {
    name: string

    implements AdbSummarizable {
        function summary(self) -> string {
            return self.name
        }
    }
}

class AdbUserHolder {
    user: AdbUser

    implements AdbHolder<AdbUser> {
        function get(self) -> Self.Item {
            return self.user
        }
    }
}

function adb_summarize<H extends AdbHolder<AdbUser>>(holder: H) -> string {
    return holder.get().summary()
}

function adb_main() -> string {
    return adb_summarize(AdbUserHolder { user: AdbUser { name: "Ada" } })
}

test "associated_type_default_typevar_satisfies_declared_bound" {
    assert.equal(adb_main(), "Ada")
}

// ─── abstract_associated_projection_bound_method_dispatch_runs ───────────────

interface ApdSomeInterface {
    function label(self) -> string throws never

    function same_label(self, other: Self) -> bool throws never {
        return self.label() == other.label()
    }
}

interface ApdHolder {
    type Item extends ApdSomeInterface

    function get(self) -> Self.Item throws never
}

class ApdWidget {
    name: string

    implements ApdSomeInterface {
        function label(self) -> string {
            return self.name
        }
    }
}

class ApdWidgetHolder {
    item: ApdWidget

    implements ApdHolder {
        type Item = ApdWidget

        function get(self) -> ApdWidget {
            return self.item
        }
    }
}

function apd_label_from_holder<H extends ApdHolder>(holder: H) -> string {
    let item = holder.get()
    return item.label()
}

function apd_holder_matches_itself<H extends ApdHolder>(holder: H) -> bool {
    let item = holder.get()
    return item.same_label(item)
}

function apd_main() -> string {
    let holder = ApdWidgetHolder { item: ApdWidget { name: "alpha" } }
    let same = if apd_holder_matches_itself(holder) { "true" } else { "false" }
    return apd_label_from_holder(holder) + ":" + same
}

test "abstract_associated_projection_bound_method_dispatch_runs" {
    assert.equal(apd_main(), "alpha:true")
}

// ─── inferred_native_generic_type_arg_from_interface_associated_return_runs ──

interface IngIterator {
    type Item

    function next(self) -> Self.Item throws never
}

class IngIntIterator {
    value: int

    implements IngIterator {
        type Item = int?

        function next(self) -> Self.Item {
            return self.value
        }
    }
}

function ing_stringify_next(iter: IngIterator<Item = int?>) -> string {
    return baml.json.to_string(iter.next())
}

function ing_main() -> string {
    return ing_stringify_next(IngIntIterator { value: 7 })
}

test "inferred_native_generic_type_arg_from_interface_associated_return_runs" {
    assert.equal(ing_main(), "7")
}

// ─── runtime_dispatch_substitutes_class_typevar_in_associated_type_binding ───

interface RdsContainer {
    type Item

    function get(self) -> Self.Item throws never
}

class RdsBox<T> {
    value: T

    implements RdsContainer {
        type Item = T

        function get(self) -> T {
            return self.value
        }
    }
}

function rds_unwrap(c: RdsContainer<Item = int>) -> int {
    return c.get()
}

function rds_main() -> int {
    return rds_unwrap(RdsBox<int> { value: 42 })
}

test "runtime_dispatch_substitutes_class_typevar_in_associated_type_binding" {
    assert.equal(rds_main(), 42)
}

// ─── runtime_dispatch_substitutes_class_typevar_inside_nested_associated_binding ─

interface RdnSource {
    type Item

    function get(self) -> Self.Item throws never
}

interface RdnOuter {
    type Inner

    function inner(self) -> Self.Inner throws never
}

class RdnBox<T> {
    value: T

    implements RdnSource {
        type Item = T

        function get(self) -> T {
            return self.value
        }
    }
}

class RdnOuterBox<T> {
    source: RdnSource<Item = T>

    implements RdnOuter {
        type Inner = RdnSource<Item = T>

        function inner(self) -> RdnSource<Item = T> {
            return self.source
        }
    }
}

function rdn_unwrap(outer: RdnOuter<Inner = RdnSource<Item = int>>) -> int {
    return outer.inner().get()
}

function rdn_main() -> int {
    return rdn_unwrap(RdnOuterBox<int> { source: RdnBox<int> { value: 42 } })
}

test "runtime_dispatch_substitutes_class_typevar_inside_nested_associated_binding" {
    assert.equal(rdn_main(), 42)
}

// ─── runtime_match_filters_by_associated_type_binding ────────────────────────

interface RmfSource {
    type Item

    function get(self) -> Self.Item throws never
}

class RmfIntSource {
    value: int

    implements RmfSource {
        type Item = int

        function get(self) -> int {
            return self.value
        }
    }
}

class RmfStringSource {
    value: string

    implements RmfSource {
        type Item = string

        function get(self) -> string {
            return self.value
        }
    }
}

function rmf_score(source: RmfSource<Item = int> | RmfSource<Item = string>) -> int {
    return match (source) {
        let ints: RmfSource<Item = int> => ints.get(),
        _ => 0,
    }
}

function rmf_main() -> int {
    return rmf_score(RmfStringSource { value: "no" }) + rmf_score(RmfIntSource { value: 7 })
}

test "runtime_match_filters_by_associated_type_binding" {
    assert.equal(rmf_main(), 7)
}

// ─── runtime_destructure_filters_by_associated_type_binding ──────────────────

interface RdfSource {
    type Item

    value: Self.Item
}

class RdfIntSource {
    value: int

    implements RdfSource {
        type Item = int
    }
}

class RdfStringSource {
    value: string

    implements RdfSource {
        type Item = string
    }
}

function rdf_score(source: RdfSource<Item = int> | RdfSource<Item = string>) -> int {
    return match (source) {
        RdfSource<Item = int> { value } => value,
        _ => 0,
    }
}

function rdf_main() -> int {
    return rdf_score(RdfStringSource { value: "no" }) + rdf_score(RdfIntSource { value: 9 })
}

test "runtime_destructure_filters_by_associated_type_binding" {
    assert.equal(rdf_main(), 9)
}

// ─── reflection_implements_respects_associated_type_bindings ─────────────────

interface RfiSource {
    type Item
}

class RfiIntSource {
    implements RfiSource {
        type Item = int
    }
}

class RfiStringSource {
    implements RfiSource {
        type Item = string
    }
}

function rfi_main() -> int {
    let score = 0
    if reflect.type_of<RfiIntSource>().implements(reflect.type_of<RfiSource<Item = int>>()) {
        score = score + 1
    }
    if reflect.type_of<RfiIntSource>().implements(reflect.type_of<RfiSource<Item = string>>()) {
        score = score + 10
    }
    if reflect.type_of<RfiStringSource>().implements(reflect.type_of<RfiSource<Item = string>>()) {
        score = score + 100
    }
    if reflect.type_of<RfiStringSource>().implements(reflect.type_of<RfiSource<Item = int>>()) {
        score = score + 1000
    }
    return score
}

test "reflection_implements_respects_associated_type_bindings" {
    assert.equal(rfi_main(), 101)
}

// ─── reflection_resolves_generic_class_associated_type_binding ───────────────
//
// A generic class's `type Item = T` binding resolves precisely at the
// instantiation: `RfgBox<int>` *is* `RfgSource<Item = int>` (consistent with
// runtime dispatch and with how Rust resolves the projection), but it is
// *not* `RfgSource<Item = string>`.

interface RfgSource {
    type Item
}

class RfgBox<T> {
    value: T

    implements RfgSource {
        type Item = T
    }
}

function rfg_main() -> int {
    let box_int = reflect.type_of<RfgBox<int>>()
    let int_source = reflect.type_of<RfgSource<Item = int>>()
    let string_source = reflect.type_of<RfgSource<Item = string>>()

    let score = 0
    if box_int.implements(int_source) {
        score = score + 1
    }
    if box_int.implements(string_source) {
        score = score + 10
    }
    if string_source.implemented_by(box_int) {
        score = score + 100
    }
    return score
}

// `RfgBox<int>.implements(RfgSource<Item = int>)` holds (+1); the two
// `Item = string` membership checks are correctly false.
test "reflection_resolves_generic_class_associated_type_binding" {
    assert.equal(rfg_main(), 1)
}

// ─── reflection_respects_generic_interface_impl_bound ────────────────────────
//
// An impl-level bound that is itself a generic interface carrying an
// associated binding (`T extends RgbSource<Item = int>`). The runtime
// resolver must discharge it as a nested obligation *at that exact
// instantiation*: a type implementing `RgbSource<Item = int>` satisfies the
// blanket impl, while one implementing `RgbSource<Item = string>` does not.

interface RgbSource {
    type Item
}

class RgbIntSource {
    implements RgbSource {
        type Item = int
    }
}

class RgbStrSource {
    implements RgbSource {
        type Item = string
    }
}

interface RgbIntSourced {}
implement<T extends RgbSource<Item = int>> RgbIntSourced for T {}

function rgb_main() -> int {
    let score = 0
    if reflect.type_of<RgbIntSource>().implements(reflect.type_of<RgbIntSourced>()) {
        score = score + 1
    }
    if reflect.type_of<RgbStrSource>().implements(reflect.type_of<RgbIntSourced>()) {
        score = score + 10
    }
    return score
}

test "reflection_respects_generic_interface_impl_bound" {
    assert.equal(rgb_main(), 1)
}

// ─── reflection_respects_generic_interface_impl_bound_plain_args ─────────────
//
// The plain-args form of a generic-interface bound (`T extends
// RpaContainer<int>`, an interface *type argument* rather than an associated
// binding). The resolver substitutes the bound's args and requires the
// type-arg to implement the interface at exactly that instantiation.

interface RpaContainer<T> {}
class RpaIntBox {
    implements RpaContainer<int> {}
}
class RpaStrBox {
    implements RpaContainer<string> {}
}

interface RpaNeedsIntContainer {}
implement<T extends RpaContainer<int>> RpaNeedsIntContainer for T {}

function rpa_main() -> int {
    let score = 0
    if reflect.type_of<RpaIntBox>().implements(reflect.type_of<RpaNeedsIntContainer>()) {
        score = score + 1
    }
    if reflect.type_of<RpaStrBox>().implements(reflect.type_of<RpaNeedsIntContainer>()) {
        score = score + 10
    }
    return score
}

test "reflection_respects_generic_interface_impl_bound_plain_args" {
    assert.equal(rpa_main(), 1)
}

// ─── reflection_literal_type_uses_concrete_base_impls ────────────────────────
//
// A literal type uses its concrete type's impls: `1` is an `int`, so
// reflection normalizes it to its concrete base before consulting the
// registry. It must answer exactly as `type_of<int>()` does.

interface RltDebuggable {
    function debug(self) -> string throws never
}
implements RltDebuggable for int {
    function debug(self) -> string { return "int" }
}

function rlt_main() -> int {
    let score = 0
    if reflect.type_of<1>().implements(reflect.type_of<RltDebuggable>()) {
        score = score + 1
    }
    if reflect.type_of<int>().implements(reflect.type_of<RltDebuggable>()) {
        score = score + 10
    }
    return score
}

test "reflection_literal_type_uses_concrete_base_impls" {
    assert.equal(rlt_main(), 11)
}

// ─── reflection_enum_variant_type_uses_concrete_base_impls ───────────────────
//
// `RevColor.Red` as a type uses `RevColor`'s impls — reflection normalizes
// the enum-variant type to its enum base before consulting the registry.

interface RevNamed {
    function name(self) -> string throws never
}
enum RevColor { Red  Green }
implements RevNamed for RevColor {
    function name(self) -> string { return "color" }
}

function rev_main() -> int {
    let score = 0
    if reflect.type_of<RevColor.Red>().implements(reflect.type_of<RevNamed>()) {
        score = score + 1
    }
    if reflect.type_of<RevColor>().implements(reflect.type_of<RevNamed>()) {
        score = score + 10
    }
    return score
}

test "reflection_enum_variant_type_uses_concrete_base_impls" {
    assert.equal(rev_main(), 11)
}
