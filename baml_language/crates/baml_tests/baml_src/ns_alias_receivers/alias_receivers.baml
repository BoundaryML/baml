// A method reached through a receiver whose static type is spelled with an
// alias (`type Key = string[]`, then `k.join(".")`) runs with the frame of the
// type the alias names: `Array<T>.join` gets `T = string`, `Box<T>.kind` gets
// `T = int`, and a matched impl's method (`implement<T> Describe for Box<T>`)
// gets the impl's `T = int`. Each method below reads its type parameters back
// — through a pattern over `T[]` (`join`) or through reflection — so a frame
// seeded without the receiver's class arguments cannot pass ("template
// references frame type-arg slot 0 but the frame has 0 type args").
//
// The same holds for a method taken as a VALUE (`let f = xs.join`): the bound
// method curries the receiver's class arguments, which for an array are its
// element type.

class Box<T> {
    item: T,

    function kind(self) -> string throws never {
        reflect.Type.of<T>().to_string()
    }

    function tagged<U>(self, other: U) -> string throws never {
        reflect.Type.of<T>().to_string() + "+" + reflect.Type.of<U>().to_string()
    }
}

type Key = string[];

type KeyAlias = Key;

type MaybeKey = Key | null;

type IntBox = Box<int>;

class KeyHolder {
    key: Key,
}

function join_key(k: Key) -> string {
    k.join(".")
}

function join_key_alias(k: KeyAlias) -> string {
    k.join(".")
}

function join_maybe_key(k: MaybeKey) -> string? {
    k?.join(".")
}

function join_held_key(h: KeyHolder) -> string {
    h.key.join(".")
}

function int_box_kind(b: IntBox) -> string {
    b.kind()
}

function int_box_tagged(b: IntBox) -> string {
    b.tagged("x")
}

function join_through_value(xs: string[]) -> string {
    let join = xs.join;
    join(".")
}

function join_key_through_value(k: Key) -> string {
    let join = k.join;
    join(".")
}

test "join_through_an_alias_of_an_array" {
    assert.equal(join_key(["a", "b"]), "a.b")
}

test "join_through_an_alias_of_an_alias" {
    assert.equal(join_key_alias(["a", "b", "c"]), "a.b.c")
}

test "join_through_an_optional_alias" {
    assert.equal(join_maybe_key(["a", "b"]), "a.b");
    assert.equal(join_maybe_key(null), null)
}

test "join_through_an_alias_typed_field" {
    assert.equal(join_held_key(KeyHolder { key: ["x", "y"] }), "x.y")
}

test "class_method_through_a_pinning_alias" {
    assert.equal(int_box_kind(Box { item: 3 }), "int")
}

test "class_method_own_generics_follow_the_alias_pinned_prefix" {
    assert.equal(int_box_tagged(Box { item: 3 }), "int+string")
}

test "array_method_taken_as_a_value" {
    assert.equal(join_through_value(["a", "b"]), "a.b")
}

test "array_method_taken_as_a_value_through_an_alias" {
    assert.equal(join_key_through_value(["a", "b"]), "a.b")
}

test "join_through_an_alias_typed_session_let" {
    let s = reflect.Session.new();
    s.eval(`type Key = string[]`);
    s.eval(`function make_key() -> Key { ["a", "b"] }`);
    s.eval(`let k = make_key()`);
    assert.equal(s.eval<string>(`k.join(".")`), "a.b")
}

interface Describe {
    function describe(self) -> string throws never;
}

interface Tagger {
    function tag<U>(self, other: U) -> string throws never;
}

implement<T> Describe for Box<T> {
    function describe(self) -> string throws never {
        "Box<" + reflect.Type.of<T>().to_string() + ">"
    }
}

implement<T> Tagger for Box<T> {
    function tag<U>(self, other: U) -> string throws never {
        reflect.Type.of<T>().to_string() + "+" + reflect.Type.of<U>().to_string()
    }
}

function int_box_describe(b: IntBox) -> string {
    b.describe()
}

function int_box_tag(b: IntBox) -> string {
    b.tag("x")
}

test "impl_method_through_a_pinning_alias" {
    assert.equal(int_box_describe(Box { item: 3 }), "Box<int>")
}

test "impl_method_own_generics_follow_the_alias_pinned_impl_frame" {
    assert.equal(int_box_tag(Box { item: 3 }), "int+string")
}
