// A value schema-aligned parsing builds inhabits the type it was parsed as,
// containers included: containers are never covariant, so a list parsed as
// `T[]` must be a `T[]`, even when it is empty.

enum Shade {
    Light,
    Dark,
}

class Note {
    title: string,
}

class Box<T> {
    item: T,
}

class Many<T> {
    xs: T[],
    m: map<string, T>,
}

interface Speaker {
    function speak(self) -> string throws never
}

function list_inhabits<T>(raw: string) -> bool {
    let parsed: unknown = baml.sap.parse<T[]>(raw);
    match (parsed) {
        let _list: T[] => true,
        _ => false,
    }
}

function map_inhabits<T>(raw: string) -> bool {
    let parsed: unknown = baml.sap.parse<map<string, T>>(raw);
    match (parsed) {
        let _map: map<string, T> => true,
        _ => false,
    }
}

function fields_inhabit<T>(raw: string) -> bool {
    let parsed = baml.sap.parse<Many<T>>(raw);
    let xs: unknown = parsed.xs;
    let m: unknown = parsed.m;
    let list = match (xs) {
        let _list: T[] => true,
        _ => false,
    };
    let map = match (m) {
        let _map: map<string, T> => true,
        _ => false,
    };
    list && map
}

test "a list of int | bigint is an (int | bigint)[]" {
    assert.is_true(list_inhabits<int | bigint>(`[]`));
    assert.is_true(list_inhabits<int | bigint>(`[1]`));
    assert.is_true(map_inhabits<int | bigint>(`{"a": 1}`));
    assert.is_true(fields_inhabit<int | bigint>(`{"xs": [1], "m": {}}`))
}

test "a list of an enum variant is a list of the variant" {
    assert.is_true(list_inhabits<Shade.Light>(`[]`));
    assert.is_true(list_inhabits<Shade.Light>(`["Light"]`));
    assert.is_true(map_inhabits<Shade.Light>(`{"a": "Light"}`));
    assert.is_true(fields_inhabit<Shade.Light>(`{"xs": ["Light"], "m": {}}`))
}

test "a list of an interface is a list of the interface" {
    assert.is_true(list_inhabits<Speaker>(`[]`));
    assert.is_true(map_inhabits<Speaker>(`{}`));
    assert.is_true(fields_inhabit<Speaker>(`{"xs": [], "m": {}}`));
    assert.is_true(list_inhabits<Speaker | int>(`[]`));
    assert.is_true(list_inhabits<Speaker | int>(`[3]`))
}

test "lists of other element types are lists of exactly them" {
    assert.is_true(list_inhabits<int>(`[]`));
    assert.is_true(list_inhabits<bigint>(`[1]`));
    assert.is_true(list_inhabits<1 | int>(`[1, 2]`));
    assert.is_true(list_inhabits<"a" | "b">(`["a"]`));
    assert.is_true(list_inhabits<Shade>(`["Dark"]`));
    assert.is_true(list_inhabits<Note>(`[{"title": "t"}]`));
    assert.is_true(list_inhabits<Note?>(`[null]`));
    assert.is_true(list_inhabits<json>(`[1, "a"]`));
    assert.is_true(list_inhabits<Box<int | bigint>>(`[{"item": 1}]`));
    assert.is_true(list_inhabits<int[]>(`[[1]]`));
    assert.is_true(list_inhabits<map<string, int>>(`[{"a": 1}]`))
}

test "nothing parses as an interface, so its list drops every element" {
    assert.equal(baml.sap.parse<Speaker[]>(`[{"name": "x"}]`).length(), 0);
    assert.is_true(list_inhabits<Speaker>(`[{"name": "x"}]`))
}

// A union member with no JSON form parses as nothing, and keeps its place in
// the container's type: a `(uint8array | string)[]` is still one.

test "a list keeps a union member that parses as nothing" {
    assert.is_true(list_inhabits<uint8array | string>(`["a"]`));
    assert.is_true(map_inhabits<uint8array | string>(`{"a": "b"}`));
    assert.is_true(fields_inhabit<uint8array | string>(`{"xs": ["a"], "m": {}}`));
    assert.is_true(list_inhabits<never>(`[]`))
}

function parse_unmodelled<T>() -> string {
    let _ = baml.sap.parse<T>(`"text"`) catch (e) {
        baml.errors.ParseError => {
            return "ParseError";
        },
    };
    "parsed"
}

test "parse throws for a type it cannot model" {
    assert.equal(
        [parse_unmodelled<uint8array>(), parse_unmodelled<never>(), parse_unmodelled<unknown>()]
            .join(","),
        "ParseError,ParseError,ParseError",
    )
}


class AnyX {
    x: int,
}

class OneX {
    x: 1,
}

class MaybeX {
    x: int?,
}

function is_one_list(value: unknown) -> bool {
    match (value) {
        let _list: 1[] => true,
        _ => false,
    }
}

function x_class(value: unknown) -> string {
    match (value) {
        let _c: OneX => "OneX",
        let _c: AnyX => "AnyX",
        let _c: MaybeX => "MaybeX",
        _ => "other",
    }
}

test "a literal list member beats an int list member in either order" {
    assert.is_true(is_one_list(baml.sap.parse<int[] | 1[]>(`[1, 1]`)));
    assert.is_true(is_one_list(baml.sap.parse<1[] | int[]>(`[1, 1]`)))
}

test "a literal class field beats an int field in either order" {
    assert.equal(x_class(baml.sap.parse<AnyX | OneX>(`{"x": 1}`)), "OneX");
    assert.equal(x_class(baml.sap.parse<OneX | AnyX>(`{"x": 1}`)), "OneX")
}

test "nullability is not ranked, so the leftmost member wins" {
    assert.equal(x_class(baml.sap.parse<AnyX | MaybeX>(`{"x": 1}`)), "AnyX");
    assert.equal(x_class(baml.sap.parse<MaybeX | AnyX>(`{"x": 1}`)), "MaybeX")
}
