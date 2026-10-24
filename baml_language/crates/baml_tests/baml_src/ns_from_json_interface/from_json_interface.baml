// Tests for the `baml.FromJson` interface and the `baml.json.to<T>` driver
// (the deserialize counterpart of `baml.ToJson` / `baml.json.from`).
//
// `baml.json.to<T>(j)` decodes a `json` value into a `T`:
//   - if `T` has an `implements baml.FromJson` override, construct via it;
//   - otherwise decode structurally (per-field).
// The auto-derived `from_json` still coexists (F1 is additive), so the
// structural path is still provided by it. These tests pin the decoded result.
//
// `Type.from_json(j)` is static-call sugar for `baml.json.to<Type>(j)`; there
// is no synthesized `from_json` method on the class.
//
// Tests that bind locals (`let`) go through a top-level helper function to
// avoid the test-block local-boxing VM bug; the pure call chain is asserted
// inline.
//
// Different FromJson implementations require unique class names in the shared
// namespace: `Temp` (reads "c") and `TempCelsius` (reads "celsius").
//
// The remaining Rust diagnostic test is `empty_implementor_is_rejected`.

// ─── shared declarations ──────────────────────────────────────────────────────

// Implementor whose override reads the `"c"` field rather than a structural
// `celsius` field.
class Temp {
    celsius: float,
    implements baml.FromJson {
        function from_json(j: baml.json.json) -> Self throws baml.json.DecodeError {
            Temp { celsius: baml.json.to<float>(baml.json.field(j, "c")) }
        }
    }
}

// Plain class (no FromJson impl) — decodes structurally.
class User {
    name: string,
    age: int,
}

// This signature compiles only if the static sugar charges exactly
// DecodeError rather than an unaccounted callee error or ParseError.
function decode_user_static(j: baml.json.json) -> User throws baml.json.DecodeError {
    User.from_json(j)
}

test "static_from_json_charges_only_json_decode_error" {
    let user = decode_user_static({ "name": "Ada", "age": 30 });
    assert.equal(user.name, "Ada")
}

// ─── to_dispatches_fromjson_override ──────────────────────────────────────────
// `baml.json.to<Temp>(j)` resolves Temp's `baml.FromJson` override, which
// reads the `"c"` field rather than a structural `celsius` field.

function to_dispatches_fromjson_override_fn() -> float throws baml.json.DecodeError {
    let j: baml.json.json = { "c": 20.5 };
    let t: Temp = baml.json.to<Temp>(j);
    t.celsius
}

test "to_dispatches_fromjson_override" {
    assert.equal(to_dispatches_fromjson_override_fn(), 20.5)
}

// ─── to_decodes_non_implementor_structurally ──────────────────────────────────
// A plain class (no FromJson impl) decodes structurally via `baml.json.to`.

function to_decodes_non_implementor_structurally_fn() -> string throws baml.json.DecodeError {
    let j: baml.json.json = { "name": "Ada", "age": 30 };
    let u: User = baml.json.to<User>(j);
    u.name
}

test "to_decodes_non_implementor_structurally" {
    assert.equal(to_decodes_non_implementor_structurally_fn(), "Ada")
}

// ─── to_decodes_primitive ─────────────────────────────────────────────────────

test "to_decodes_primitive" {
    assert.equal(baml.json.to<int>(42), 42)
}

// ─── from_to_roundtrip_with_override ──────────────────────────────────────────
// `json.to<T>(json.from(x))` round-trips, dispatching the FromJson override
// (this implementor's override reads "celsius", matching the structural
// rendering `baml.json.from` produces).

class TempCelsius {
    celsius: float,
    implements baml.FromJson {
        function from_json(j: baml.json.json) -> Self throws baml.json.DecodeError {
            TempCelsius { celsius: baml.json.to<float>(baml.json.field(j, "celsius")) }
        }
    }
}

function from_to_roundtrip_with_override_fn(
) -> bool throws baml.json.SerializationError | baml.json.DecodeError {
    let original: TempCelsius = TempCelsius { celsius: 36.6 };
    let j: baml.json.json = baml.json.from(original);
    let decoded: TempCelsius = baml.json.to<TempCelsius>(j);
    decoded.celsius == 36.6
}

test "from_to_roundtrip_with_override" {
    assert.is_true(from_to_roundtrip_with_override_fn())
}

// ─── nested_field_override_is_honored ─────────────────────────────────────────
// `Outer` has no FromJson impl, so it decodes via the per-field Rust decoder;
// its `Inner` field implements FromJson and must decode via its override
// (which reads "wrapped", not a structural "v").

class Inner {
    v: int,
    implements baml.FromJson {
        function from_json(j: baml.json.json) -> Self throws baml.json.DecodeError {
            Inner { v: baml.json.to<int>(baml.json.field(j, "wrapped")) }
        }
    }
}

class Outer {
    inner: Inner,
    tag: string,
}

function nested_field_override_is_honored_fn() -> int throws baml.json.DecodeError {
    let j: baml.json.json = { "inner": { "wrapped": 7 }, "tag": "x" };
    let o: Outer = baml.json.to<Outer>(j);
    o.inner.v
}

test "nested_field_override_is_honored" {
    assert.equal(nested_field_override_is_honored_fn(), 7)
}

// ── The `Type.from_json(j)` static-call sugar ─────────────────────────────────
// These exercise the sugar form directly (the tests above use the
// `baml.json.to` driver).

// ─── static_from_json_decodes_non_implementor ─────────────────────────────────
// `User.from_json(j)` (no FromJson impl) desugars to `baml.json.to<User>(j)`
// and decodes structurally.

function static_from_json_decodes_non_implementor_fn() -> string throws baml.json.DecodeError {
    let j: baml.json.json = { "name": "Ada", "age": 30 };
    let u: User = User.from_json(j);
    u.name
}

test "static_from_json_decodes_non_implementor" {
    assert.equal(static_from_json_decodes_non_implementor_fn(), "Ada")
}

// ─── static_from_json_dispatches_override ─────────────────────────────────────
// An implementor's `Temp.from_json(j)` resolves Temp's own `baml.FromJson`
// override (not the sugar) — it reads "c", not a structural "celsius".

function static_from_json_dispatches_override_fn() -> float throws baml.json.DecodeError {
    let j: baml.json.json = { "c": 20.5 };
    let t: Temp = Temp.from_json(j);
    t.celsius
}

test "static_from_json_dispatches_override" {
    assert.equal(static_from_json_dispatches_override_fn(), 20.5)
}

// ─── static_from_json_threads_generic_type_arg ────────────────────────────────
// `Box<int>.from_json(j)` — the `<int>` parses as the call's type args and is
// applied to the receiver, so it desugars to `baml.json.to<Box<int>>(j)` and
// the field decodes as int (the case the `-> Self` delegate could not thread).

class Box<T> {
    value: T,
}

function static_from_json_threads_generic_type_arg_fn() -> int throws baml.json.DecodeError {
    let j: baml.json.json = { "value": 42 };
    let b: Box<int> = Box<int>.from_json(j);
    b.value
}

test "static_from_json_threads_generic_type_arg" {
    assert.equal(static_from_json_threads_generic_type_arg_fn(), 42)
}

test "empty_implementor_is_rejected" {
    root.compiler.AssertHasMessage(
        {
            "empty_implementor_is_rejected.baml": `
                class Point {
                    x int
                    y int
                    implements baml.FromJson {}
                }
                function main() -> int { 1 }
            `,
        },
        "from_json",
    )
}

// ─── nesting depth ────────────────────────────────────────────────────────────

// A structural decode walks as deep as the document nests: the decoder keeps
// the containers it is inside on a stack of its own, so nesting costs no call
// frames. 127 levels is the deepest document `baml.json.parse` accepts.
class DeepLink {
    next: DeepLink?,
}

function deep_link_document(depth: int) -> string {
    let open = "";
    let close = "";
    let i = 0;
    while (i < depth) {
        open = open + "{\"next\":";
        close = close + "}";
        i = i + 1;
    }
    open + "null" + close
}

function decode_nests_as_deep_as_the_document_fn(
    depth: int
) -> int throws baml.json.ParseError | baml.json.DecodeError {
    let link = baml.json.to<DeepLink>(baml.json.parse(deep_link_document(depth)));
    let links = 1;
    let next = link.next;
    while (next != null) {
        links = links + 1;
        next = next.next;
    }
    links
}

test "decode_nests_as_deep_as_the_document" {
    assert.equal(decode_nests_as_deep_as_the_document_fn(127), 127)
}

// ─── a provided `from_json` wins wherever the walk meets its type ─────────────
// `to<T>` mirrors `==`: a provided `baml.FromJson` wins and anything else
// decodes structurally, recursively. An enum's override is honored in every
// position a class's is — root, field, generic field, optional, list, map.

enum Shade {
    Light,
    Dark,
}

// Reads any string as `Dark`: the structural reading of `"Light"` is `Light`.
implements baml.FromJson for Shade {
    function from_json(j: baml.json.json) -> Self throws never {
        Shade.Dark
    }
}

class Paint {
    shade: Shade,
}

class Crate<T> {
    item: T,
}

class Palette {
    main: Shade?,
    all: Shade[],
    named: map<string, Shade>,
    paints: Paint[],
}

function enum_override_at_root_fn() -> Shade throws baml.json.DecodeError {
    baml.json.to<Shade>("Light")
}

test "enum_override_at_root" {
    assert.equal(enum_override_at_root_fn(), Shade.Dark)
}

function enum_override_in_field_fn() -> Shade throws baml.json.DecodeError {
    baml.json.to<Paint>({ "shade": "Light" }).shade
}

test "enum_override_in_field" {
    assert.equal(enum_override_in_field_fn(), Shade.Dark)
}

function enum_override_in_generic_field_fn() -> Shade throws baml.json.DecodeError {
    baml.json.to<Crate<Shade>>({ "item": "Light" }).item
}

test "enum_override_in_generic_field" {
    assert.equal(enum_override_in_generic_field_fn(), Shade.Dark)
}

function enum_override_in_containers_fn() -> string throws baml.json.DecodeError {
    let palette = baml.json.to<Palette>({
        "main": "Light",
        "all": ["Light"],
        "named": { "k": "Light" },
        "paints": [{ "shade": "Light" }],
    });
    string.from(palette.main) + "|" + string.from(palette.all) + "|"
        + string.from(palette.named["k"]) + "|" + string.from(palette.paints[0].shade)
}

test "enum_override_in_containers" {
    assert.equal(enum_override_in_containers_fn(), "Dark|[Dark]|Dark|Dark")
}

// An absent optional is `null`, never handed to the override.
function enum_override_absent_optional_fn() -> Shade? throws baml.json.DecodeError {
    baml.json.to<Palette>({ "all": [], "named": {}, "paints": [] }).main
}

test "enum_override_absent_optional" {
    assert.equal(enum_override_absent_optional_fn(), null)
}

// Every override-honoring entry is the same walk: `from_json<T>`,
// `deserialize<T>`, and the `T.from_json(j)` sugar.
function enum_override_through_every_entry_fn() -> string throws baml.json.DecodeError | baml.json.ParseError {
    let a = baml.json.to<Paint>({ "shade": "Light" }).shade;
    let b = baml.json.decode<Paint>("{\"shade\": \"Light\"}").shade;
    let c = Paint.from_json({ "shade": "Light" }).shade;
    string.from(a) + string.from(b) + string.from(c)
}

test "enum_override_through_every_entry" {
    assert.equal(enum_override_through_every_entry_fn(), "DarkDarkDark")
}

// ─── a union member is chosen by shape, then decoded by the rule ──────────────
// A provided `from_json` that throws unwinds the whole walk, so a member
// cannot be tried through its override and fallen through to the next. The
// member the value fits best structurally is the one; it then decodes
// through the override-honoring walk.

function union_member_then_override_fn() -> string throws baml.json.DecodeError {
    // `"Light"` fits `Shade` structurally; the chosen member's override runs.
    string.from(baml.json.to<Shade | int>("Light")) + "|" + string.from(baml.json.to<Shade | int>(3))
}

test "union_member_then_override" {
    assert.equal(union_member_then_override_fn(), "Dark|3")
}

// `Temp`'s override reads `"c"`, but its shape is `{ celsius }`: inside a
// union the override's reading does not make `Temp` the member.
function union_membership_is_structural_fn() -> string {
    {
        let t = baml.json.to<Temp | int>({ "c": 20.5 });
        "decoded"
    } catch (e) {
        baml.json.DecodeError => "no member: " + e.message,
    }
}

test "union_membership_is_structural" {
    assert.equal(
        union_membership_is_structural_fn(),
        "no member: no union member matched: `from_json_interface.Temp`: missing required field `celsius`; `int`: expected integer",
    )
}

// ─── a value that contains itself ─────────────────────────────────────────────
// The walk refuses a JSON object or array it already has open instead of
// descending into it forever.

class Link {
    next: Link?,
    name: string,
}

class Ring {
    items: Ring[],
}

function cyclic_object_is_refused_fn() -> string {
    let m: map<string, baml.json.json> = { "name": "a" };
    m["next"] = m;
    {
        let link = baml.json.to<Link>(m);
        "decoded " + link.name
    } catch (e) {
        baml.json.DecodeError => e.message,
    }
}

test "cyclic_object_is_refused" {
    assert.equal(cyclic_object_is_refused_fn(), "cyclic json value: the document contains itself")
}

function cyclic_array_is_refused_fn() -> string {
    let items: baml.json.json[] = [];
    let m: map<string, baml.json.json> = { "items": items };
    items.push(m);
    {
        let ring = baml.json.to<Ring>(m);
        "decoded " + string.from(ring.items.length())
    } catch (e) {
        baml.json.DecodeError => e.message,
    }
}

test "cyclic_array_is_refused" {
    assert.equal(cyclic_array_is_refused_fn(), "cyclic json value: the document contains itself")
}

// The refusal does not depend on the position the cycle is met in: a union
// candidate and a `json`-typed leaf are converted whole, and the conversion
// refuses a container it is already inside, at that place.
function cyclic_union_candidate_is_refused_fn() -> string {
    let m: map<string, baml.json.json> = { "name": "a" };
    m["next"] = m;
    {
        let value = baml.json.to<Link | int>(m);
        "decoded"
    } catch (e) {
        baml.json.DecodeError => e.message,
    }
}

test "cyclic_union_candidate_is_refused" {
    assert.equal(cyclic_union_candidate_is_refused_fn(), "cyclic json value: the document contains itself")
}

class JsonHolder {
    data: baml.json.json,
}

function cyclic_json_leaf_is_refused_fn() -> string {
    let m: map<string, baml.json.json> = { "name": "a" };
    m["next"] = m;
    let doc: map<string, baml.json.json> = { "data": m };
    {
        let holder = baml.json.to<JsonHolder>(doc);
        "decoded"
    } catch (e) {
        baml.json.DecodeError => e.message + " at " + e.path,
    }
}

test "cyclic_json_leaf_is_refused_at_its_path" {
    assert.equal(
        cyclic_json_leaf_is_refused_fn(),
        "cyclic json value: the document contains itself at .data[\"next\"]"
    )
}

// ─── error paths ──────────────────────────────────────────────────────────────
// A decode error names where it happened, from the root: the class whose
// field is missing, or the leaf whose value has the wrong shape.

class PathInner {
    items: Link[],
}

class PathOuter {
    inner: PathInner,
}

function nested_decode_error_path(doc: map<string, baml.json.json>) -> string {
    {
        let outer = baml.json.to<PathOuter>(doc);
        "decoded"
    } catch (e) {
        baml.json.DecodeError => e.message + " at " + e.path,
    }
}

test "missing_field_error_names_the_class_path" {
    let doc: map<string, baml.json.json> = {
        "inner": { "items": [{ "name": "a", "next": null }, { "next": null }] },
    };
    assert.equal(nested_decode_error_path(doc), "missing required field `name` at .inner.items[1]")
}

test "wrong_leaf_error_names_the_leaf_path" {
    let doc: map<string, baml.json.json> = {
        "inner": { "items": [{ "name": "a", "next": null }, { "name": 5, "next": null }] },
    };
    assert.is_true(nested_decode_error_path(doc).ends_with(" at .inner.items[1].name"))
}

// A document that merely repeats one object (a DAG, not a cycle) decodes.
function shared_object_is_not_a_cycle_fn() -> int throws baml.json.DecodeError {
    let leaf: map<string, baml.json.json> = { "next": null, "name": "leaf" };
    let m: map<string, baml.json.json> = { "items": [leaf, leaf] };
    baml.json.to<map<string, Link[]>>(m)["items"].length()
}

test "shared_object_is_not_a_cycle" {
    assert.equal(shared_object_is_not_a_cycle_fn(), 2)
}
