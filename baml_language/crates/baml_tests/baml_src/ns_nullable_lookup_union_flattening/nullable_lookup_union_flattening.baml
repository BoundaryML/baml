// B-1885: lookup adds null to a nullable value, including one named by aliases.
class Group {
    text: string,
}

type MaybeGroup = Group | null;

type Again = MaybeGroup | null;

function map_text(m: map<string, Group?>) -> string {
    m.get("k")?.text ?? ""
}

function array_text(xs: (Group | null)[]) -> string {
    xs.at(1)?.text ?? ""
}

function regex_text(text: string) -> string {
    let m = baml.regex.new("(a)?b").match_once(text);
    if let found: baml.regex.Match = m {
        found.groups.at(1)?.text ?? ""
    } else {
        ""
    }
}

function alias_map_text(m: map<string, Again>) -> string {
    m.get("k")?.text ?? ""
}

function alias_array_text(xs: Again[]) -> string {
    xs.at(1)?.text ?? ""
}

function nullable<T>(value: T) -> T? {
    value
}

function alias_generic_text(value: Again) -> string {
    nullable<Again>(value)?.text ?? ""
}

function alias_direct_text(value: Again) -> string {
    value?.text ?? ""
}

function alias_guard_text(value: Again) -> string {
    if (value == null) {
        ""
    } else {
        value.text
    }
}

function alias_coalesce(value: Again) -> Group {
    value ?? Group { text: "fallback" }
}

test "nullable_map_lookup" {
    assert.equal(map_text({ "k": Group { text: "hi" } }), "hi");
    assert.equal(map_text({ "k": null }), "");
    assert.equal(map_text({}), "")
}

test "nullable_array_lookup" {
    assert.equal(array_text([null, Group { text: "hi" }]), "hi");
    assert.equal(array_text([null, null]), "");
    assert.equal(array_text([]), "")
}

test "nullable_regex_capture_lookup" {
    assert.equal(regex_text("xab"), "a");
    assert.equal(regex_text("b"), "");
    assert.equal(regex_text("x"), "")
}

test "aliased_nullable_lookups" {
    assert.equal(alias_map_text({ "k": Group { text: "hi" } }), "hi");
    assert.equal(alias_map_text({ "k": null }), "");
    assert.equal(alias_map_text({}), "");
    assert.equal(alias_array_text([null, Group { text: "hi" }]), "hi");
    assert.equal(alias_array_text([null, null]), "");
    assert.equal(alias_array_text([]), "");
    assert.equal(alias_generic_text(Group { text: "hi" }), "hi");
    assert.equal(alias_generic_text(null), "");
    assert.equal(alias_direct_text(Group { text: "hi" }), "hi");
    assert.equal(alias_direct_text(null), "");
    assert.equal(alias_guard_text(Group { text: "hi" }), "hi");
    assert.equal(alias_guard_text(null), "");
    assert.equal(alias_coalesce(Group { text: "hi" }).text, "hi");
    assert.equal(alias_coalesce(null).text, "fallback")
}

class Nested {
    child: Group?,
}

function nested_child(xs: Nested?[]) -> Group? {
    xs.at(0)?.child
}

function nested_list(m: map<string, (Group | null)[]?>) -> (Group | null)[] {
    m.get("k") ?? []
}

function nested_map(m: map<string, map<string, Group?>?>) -> map<string, Group?> {
    m.get("k") ?? {}
}

test "flattening_preserves_nested_nulls" {
    assert.equal(nested_child([Nested { child: null }]), null);
    assert.equal(nested_child([null]), null);
    assert.equal(nested_list({ "k": [null] }).at(0), null);
    assert.equal(nested_list({ "k": [null] }).length(), 1);
    assert.equal(nested_map({ "k": { "inner": null } }).length(), 1)
}

type NamedCallback = ((first: string, second: string) -> string throws never)?;

type AgainCallback = NamedCallback | null;

function call_named(callback: AgainCallback) -> string {
    callback?.(second = "b", first = "a") ?? ""
}

function concatenate(first: string, second: string) -> string throws never {
    first + second
}

test "aliased_nullable_callback_preserves_named_arguments" {
    assert.equal(call_named(concatenate), "ab");
    assert.equal(call_named(null), "")
}

type Ints = int[];

function nonnullable_alias_index(xs: Ints | string[]) -> int | string {
    xs?.[0]
}

test "unfolding_a_nonnullable_alias_does_not_add_null" {
    assert.equal(nonnullable_alias_index([7]), 7);
    assert.equal(nonnullable_alias_index(["hi"]), "hi")
}
