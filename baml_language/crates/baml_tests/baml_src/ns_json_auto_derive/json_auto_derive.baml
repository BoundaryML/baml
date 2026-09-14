// Tests for auto-derived to_json / from_json on user classes.
//
// Tests that read fields from `let` locals are moved into helper functions
// (test-block local-boxing VM bug: field access on a stored local throws
// `expected Instance, got Any`).

// ─── shared top-level class declarations ─────────────────────────────────────

class User { name string  age int }

// ─── simple_class_to_json_roundtrip ──────────────────────────────────────────

function simple_class_to_json_roundtrip_name() -> string {
    let u: User = User { name: "Ada", age: 30 };
    let j: baml.json.json = u.to_json();
    let v: User = User.from_json(j);
    v.name
}

test "simple_class_to_json_roundtrip" {
    assert.equal(simple_class_to_json_roundtrip_name(), "Ada")
}

// ─── user_to_json_override_suppresses_auto_derive ────────────────────────────

class Secret {
    value string

    implements baml.ToJson {
        function to_json(self) -> baml.json.json throws baml.json.SerializationError {
            "[redacted]"
        }
    }
}

function user_to_json_override_result() -> baml.json.json throws baml.json.SerializationError {
    let s: Secret = Secret { value: "hunter2" };
    s.to_json()
}

test "user_to_json_override_suppresses_auto_derive" {
    assert.equal(user_to_json_override_result(), "[redacted]")
}

// ─── Phase 5b.2: primitive companion bridging ─────────────────────────────────

// int.to_json() must resolve to baml.Int.to_json and return the int as json.
function int_to_json_result() -> baml.json.json {
    let n: int = 42;
    n.to_json()
}

test "int_to_json_resolves_and_executes" {
    assert.equal(int_to_json_result(), 42)
}

// float.to_json() resolves to baml.Float.to_json and returns the float.
function float_to_json_result() -> baml.json.json {
    let f: float = 2.5;
    f.to_json()
}

test "float_to_json_resolves_and_executes" {
    assert.equal(float_to_json_result(), 2.5)
}

// bool.to_json() resolves to baml.Bool.to_json and returns the bool.
function bool_to_json_result() -> baml.json.json {
    let b: bool = true;
    b.to_json()
}

test "bool_to_json_resolves_and_executes" {
    // Not `assert.is_true`: the result is a `baml.json.json`, not a `bool`.
    assert.equal(bool_to_json_result(), true)
}

// null.to_json() resolves to baml.Null.to_json and returns null.
function null_to_json_result() -> baml.json.json {
    let n: null = null;
    n.to_json()
}

test "null_to_json_resolves_and_executes" {
    assert.equal(null_to_json_result(), null)
}

// ─── Phase 5b.4: Array<T>.to_json and Map<K,V>.to_json ───────────────────────

// int[].to_json() must return a json array [1,2,3].
// Structural check: stringify the result and compare.
function array_of_int_to_json_str() -> string throws baml.json.SerializationError | baml.json.ParseError {
    let a: int[] = [1, 2, 3];
    baml.json.stringify(a.to_json())
}

test "array_of_int_to_json_returns_json_array" {
    assert.equal(array_of_int_to_json_str(), "[1,2,3]")
}

// B[] with to_json override: each element maps to 99, result is [99,99].
class BArr {
    x int

    implements baml.ToJson {
        function to_json(self) -> baml.json.json throws baml.json.SerializationError {
            99
        }
    }
}

function array_of_class_to_json_str() -> string throws baml.json.SerializationError | baml.json.ParseError {
    let bs: BArr[] = [BArr { x: 1 }, BArr { x: 2 }];
    baml.json.stringify(bs.to_json())
}

test "array_of_class_to_json_honors_override" {
    assert.equal(array_of_class_to_json_str(), "[99,99]")
}

// map<string,int>.to_json() must return a json map. Structural check via stringify.
function map_of_int_to_json_str() -> string throws baml.json.SerializationError | baml.json.ParseError {
    let m: map<string, int> = {"a": 1, "b": 2};
    baml.json.stringify(m.to_json())
}

test "map_of_int_to_json_returns_json_map" {
    // Map key insertion order is deterministic (a, b).
    assert.equal(map_of_int_to_json_str(), "{\"a\":1,\"b\":2}")
}

// ─── Phase 5b.2.3: universal TypeVar to_json / from_json ─────────────────────

class G<T> {
    items T[]
    function f(self) -> baml.json.json[] throws baml.json.SerializationError | baml.json.ParseError {
        self.items.map((x: T) -> baml.json.json throws baml.json.SerializationError | baml.json.ParseError { x.to_json() })
    }
}

test "typevar_to_json_compiles_in_generic_class" {
    // Only compilation and basic execution is verified; G.f is not called.
    assert.equal(1, 1)
}

// ─── Phase 5b.6: per-field auto-derive, override-honoring, recursive, generic ─

class C1 { x int }

function primitive_field_to_json_str() -> string throws baml.json.SerializationError | baml.json.ParseError {
    let c: C1 = C1 { x: 42 };
    let j: baml.json.json = c.to_json();
    baml.json.stringify(j)
}

test "primitive_field_to_json" {
    assert.equal(primitive_field_to_json_str(), "{\"x\":42}")
}

class C2 { xs int[] }

function array_of_primitive_to_json_str() -> string throws baml.json.SerializationError | baml.json.ParseError {
    let c: C2 = C2 { xs: [1, 2, 3] };
    let j: baml.json.json = c.to_json();
    baml.json.stringify(j)
}

test "array_of_primitive_to_json" {
    assert.equal(array_of_primitive_to_json_str(), "{\"xs\":[1,2,3]}")
}

// nested_class_override_honored_direct: A{b: B{y:7}}.to_json() → {"b":"[secret]"}
class BSecret {
    y int

    implements baml.ToJson {
        function to_json(self) -> baml.json.json throws baml.json.SerializationError {
            "[secret]"
        }
    }
}

class ASecret { b BSecret }

function nested_class_override_direct_str() -> string throws baml.json.SerializationError | baml.json.ParseError {
    let a: ASecret = ASecret { b: BSecret { y: 7 } };
    let j: baml.json.json = a.to_json();
    baml.json.stringify(j)
}

test "nested_class_override_honored_direct" {
    assert.equal(nested_class_override_direct_str(), "{\"b\":\"[secret]\"}")
}

// nested_class_override_honored_in_array: A{items:[B{y:1},B{y:2}]}.to_json() → {"items":[0,0]}
class BZero {
    y int

    implements baml.ToJson {
        function to_json(self) -> baml.json.json throws baml.json.SerializationError {
            0
        }
    }
}

class AItems { items BZero[] }

function nested_class_override_in_array_str() -> string throws baml.json.SerializationError | baml.json.ParseError {
    let a: AItems = AItems { items: [BZero { y: 1 }, BZero { y: 2 }] };
    let j: baml.json.json = a.to_json();
    baml.json.stringify(j)
}

test "nested_class_override_honored_in_array" {
    assert.equal(nested_class_override_in_array_str(), "{\"items\":[0,0]}")
}

// nested_class_override_honored_in_map: A{lookup:{k1:B{v:1},k2:B{v:2}}}.to_json() → both values "B"
class BStr {
    v int

    implements baml.ToJson {
        function to_json(self) -> baml.json.json throws baml.json.SerializationError {
            "B"
        }
    }
}

class ALookup { lookup map<string, BStr> }

function nested_class_override_in_map_str() -> string throws baml.json.SerializationError | baml.json.ParseError {
    let a: ALookup = ALookup { lookup: {"k1": BStr { v: 1 }, "k2": BStr { v: 2 }} };
    let j: baml.json.json = a.to_json();
    baml.json.stringify(j)
}

test "nested_class_override_honored_in_map" {
    // Verify both map values serialize to "B" via the override.
    // Map key insertion order is deterministic (k1, k2).
    assert.equal(nested_class_override_in_map_str(), "{\"lookup\":{\"k1\":\"B\",\"k2\":\"B\"}}")
}

// recursive_class_to_json: Tree{value:1, children:[Tree{value:2, children:[]}]}.to_json()
class Tree {
    value int
    children Tree[]
}

function recursive_class_to_json_str() -> string throws baml.json.SerializationError | baml.json.ParseError {
    let leaf: Tree = Tree { value: 2, children: [] };
    let root: Tree = Tree { value: 1, children: [leaf] };
    let j: baml.json.json = root.to_json();
    baml.json.stringify(j)
}

test "recursive_class_to_json" {
    assert.equal(
        recursive_class_to_json_str(),
        "{\"value\":1,\"children\":[{\"value\":2,\"children\":[]}]}",
    )
}

// generic_class_to_json_concrete: Box<int>{value:42}.to_json() → {"value":42}
class Box1<T> { value T }

function generic_class_to_json_concrete_str() -> string throws baml.json.SerializationError | baml.json.ParseError {
    let b: Box1<int> = Box1<int> { value: 42 };
    let j: baml.json.json = b.to_json();
    baml.json.stringify(j)
}

test "generic_class_to_json_concrete" {
    assert.equal(generic_class_to_json_concrete_str(), "{\"value\":42}")
}

// generic_class_to_json_array_typearg: Box<int[]>{value:[1,2,3]}.to_json() → {"value":[1,2,3]}
class Box2<T> { value T }

function generic_class_to_json_array_typearg_str() -> string throws baml.json.SerializationError | baml.json.ParseError {
    let b: Box2<int[]> = Box2<int[]> { value: [1, 2, 3] };
    let j: baml.json.json = b.to_json();
    baml.json.stringify(j)
}

test "generic_class_to_json_array_typearg" {
    assert.equal(generic_class_to_json_array_typearg_str(), "{\"value\":[1,2,3]}")
}

// generic_class_honors_override_through_typevar: Box<Secret>{value:Secret{...}}.to_json()
class SecretRedact {
    data string

    implements baml.ToJson {
        function to_json(self) -> baml.json.json throws baml.json.SerializationError {
            "[redacted]"
        }
    }
}

class Box3<T> { value T }

function generic_class_honors_override_str() -> string throws baml.json.SerializationError | baml.json.ParseError {
    let b: Box3<SecretRedact> = Box3<SecretRedact> { value: SecretRedact { data: "hunter2" } };
    let j: baml.json.json = b.to_json();
    baml.json.stringify(j)
}

test "generic_class_honors_override_through_typevar" {
    assert.equal(generic_class_honors_override_str(), "{\"value\":\"[redacted]\"}")
}

// generic_class_with_optional_typevar_field: Box<int>{value:42}.to_json() → {"value":42}
class Box4<T> { value T? }

function generic_class_optional_typevar_str() -> string throws baml.json.SerializationError | baml.json.ParseError {
    let b: Box4<int> = Box4<int> { value: 42 };
    let j: baml.json.json = b.to_json();
    baml.json.stringify(j)
}

test "generic_class_with_optional_typevar_field" {
    assert.equal(generic_class_optional_typevar_str(), "{\"value\":42}")
}

// generic_class_with_optional_typevar_field_null: Box<int>{value:null}.to_json() → {"value":null}
function generic_class_optional_typevar_null_str() -> string throws baml.json.SerializationError | baml.json.ParseError {
    let b: Box4<int> = Box4<int> { value: null };
    let j: baml.json.json = b.to_json();
    baml.json.stringify(j)
}

test "generic_class_with_optional_typevar_field_null" {
    assert.equal(generic_class_optional_typevar_null_str(), "{\"value\":null}")
}

// generic_class_with_optional_typevar_field_honors_override
class SecretRedact2 {
    data string

    implements baml.ToJson {
        function to_json(self) -> baml.json.json throws baml.json.SerializationError {
            "[redacted]"
        }
    }
}

class Box5<T> { value T? }

function generic_class_optional_override_str() -> string throws baml.json.SerializationError | baml.json.ParseError {
    let b: Box5<SecretRedact2> = Box5<SecretRedact2> { value: SecretRedact2 { data: "hunter2" } };
    let j: baml.json.json = b.to_json();
    baml.json.stringify(j)
}

test "generic_class_with_optional_typevar_field_honors_override" {
    assert.equal(generic_class_optional_override_str(), "{\"value\":\"[redacted]\"}")
}

// ─── Phase 5c: per-field auto-derive from_json with override-honoring ─────────

class C3 { x int }

function primitive_field_from_json_val() -> int throws baml.json.ParseError | baml.json.DecodeError {
    let j: baml.json.json = baml.json.parse("{\"x\":42}");
    let c: C3 = C3.from_json(j);
    c.x
}

test "primitive_field_from_json" {
    assert.equal(primitive_field_from_json_val(), 42)
}

class C4 { xs int[] }

function array_of_primitive_from_json_sum() -> int throws baml.json.ParseError | baml.json.DecodeError {
    let j: baml.json.json = baml.json.parse("{\"xs\":[1,2,3]}");
    let c: C4 = C4.from_json(j);
    c.xs[0] + c.xs[1] + c.xs[2]
}

test "array_of_primitive_from_json" {
    assert.equal(array_of_primitive_from_json_sum(), 6)
}

// nested_class_override_honored_direct_from_json: B.from_json override returns B{y:99}
class BOverride {
    y int

    implements baml.FromJson {
        function from_json(j: baml.json.json) -> Self throws baml.json.DecodeError {
            BOverride { y: 99 }
        }
    }
}

class AOverride { b BOverride }

function nested_class_override_direct_from_json_val() -> int throws baml.json.ParseError | baml.json.DecodeError {
    let j: baml.json.json = baml.json.parse("{\"b\":{\"y\":7}}");
    let a: AOverride = AOverride.from_json(j);
    a.b.y
}

test "nested_class_override_honored_direct_from_json" {
    assert.equal(nested_class_override_direct_from_json_val(), 99)
}

// nested_class_override_honored_in_array_from_json: B.from_json always returns B{y:0}
class BZeroFrom {
    y int

    implements baml.FromJson {
        function from_json(j: baml.json.json) -> Self throws baml.json.DecodeError {
            BZeroFrom { y: 0 }
        }
    }
}

class AItemsFrom { items BZeroFrom[] }

function nested_class_override_array_from_json_sum() -> int throws baml.json.ParseError | baml.json.DecodeError {
    let j: baml.json.json = baml.json.parse("{\"items\":[{\"y\":1},{\"y\":2}]}");
    let a: AItemsFrom = AItemsFrom.from_json(j);
    a.items[0].y + a.items[1].y
}

test "nested_class_override_honored_in_array_from_json" {
    assert.equal(nested_class_override_array_from_json_sum(), 0)
}

// nested_class_override_honored_in_map_from_json: B.from_json always returns B{v:0}
class BMapFrom {
    v int

    implements baml.FromJson {
        function from_json(j: baml.json.json) -> Self throws baml.json.DecodeError {
            BMapFrom { v: 0 }
        }
    }
}

class ALookupFrom { lookup map<string, BMapFrom> }

function nested_class_override_map_from_json_sum() -> int throws baml.json.ParseError | baml.json.DecodeError {
    let j: baml.json.json = baml.json.parse("{\"lookup\":{\"k1\":{\"v\":7},\"k2\":{\"v\":9}}}");
    let a: ALookupFrom = ALookupFrom.from_json(j);
    a.lookup["k1"].v + a.lookup["k2"].v
}

test "nested_class_override_honored_in_map_from_json" {
    assert.equal(nested_class_override_map_from_json_sum(), 0)
}

// recursive_class_from_json
function recursive_class_from_json_sum() -> int throws baml.json.SerializationError | baml.json.ParseError | baml.json.DecodeError {
    let leaf: Tree = Tree { value: 2, children: [] };
    let root: Tree = Tree { value: 1, children: [leaf] };
    let s: string = baml.json.stringify(root.to_json());
    let j: baml.json.json = baml.json.parse(s);
    let parsed: Tree = Tree.from_json(j);
    parsed.value + parsed.children[0].value
}

test "recursive_class_from_json" {
    assert.equal(recursive_class_from_json_sum(), 3)
}

// generic_class_from_json_concrete: Box<int>.from_json(parse("{\"value\":42}"))
class Box6<T> { value T }

function generic_class_from_json_concrete_val() -> int throws baml.json.ParseError | baml.json.DecodeError {
    let j: baml.json.json = baml.json.parse("{\"value\":42}");
    let b: Box6<int> = Box6<int>.from_json(j);
    b.value
}

test "generic_class_from_json_concrete" {
    assert.equal(generic_class_from_json_concrete_val(), 42)
}

// generic_class_honors_override_through_typevar_from_json: Box<Secret>.from_json dispatches Secret.from_json
class SecretReset {
    data string

    implements baml.FromJson {
        function from_json(j: baml.json.json) -> Self throws baml.json.DecodeError {
            SecretReset { data: "[reset]" }
        }
    }
}

class Box7<T> { value T }

function generic_class_honors_override_from_json_val() -> string throws baml.json.ParseError | baml.json.DecodeError {
    let j: baml.json.json = baml.json.parse("{\"value\":{\"data\":\"hunter2\"}}");
    let b: Box7<SecretReset> = Box7<SecretReset>.from_json(j);
    b.value.data
}

test "generic_class_honors_override_through_typevar_from_json" {
    assert.equal(generic_class_honors_override_from_json_val(), "[reset]")
}

// ─── Phase 5c: additional coverage ───────────────────────────────────────────

// optional_class_override_honored_from_json_non_null: A{b:B?} with B.from_json override
class BOptFrom {
    y int

    implements baml.FromJson {
        function from_json(j: baml.json.json) -> Self throws baml.json.DecodeError {
            BOptFrom { y: 99 }
        }
    }
}

class AOptFrom { b BOptFrom? }

function optional_class_override_non_null_val() -> int throws baml.json.ParseError | baml.json.DecodeError {
    let j: baml.json.json = baml.json.parse("{\"b\":{\"y\":7}}");
    let a: AOptFrom = AOptFrom.from_json(j);
    match (a.b) {
        null => -1,
        let inner: BOptFrom => inner.y,
    }
}

test "optional_class_override_honored_from_json_non_null" {
    assert.equal(optional_class_override_non_null_val(), 99)
}

// optional_class_override_honored_from_json_null: same but b: null → -1
function optional_class_override_null_val() -> int throws baml.json.ParseError | baml.json.DecodeError {
    let j: baml.json.json = baml.json.parse("{\"b\":null}");
    let a: AOptFrom = AOptFrom.from_json(j);
    match (a.b) {
        null => -1,
        let inner: BOptFrom => inner.y,
    }
}

test "optional_class_override_honored_from_json_null" {
    assert.equal(optional_class_override_null_val(), -1)
}

// optional_primitive_field_from_json: C{x:int?}, tests both non-null and null paths
class C5 { x int? }

function optional_primitive_field_from_json_sum() -> int throws baml.json.ParseError | baml.json.DecodeError {
    let j1: baml.json.json = baml.json.parse("{\"x\":42}");
    let c1: C5 = C5.from_json(j1);
    let j2: baml.json.json = baml.json.parse("{\"x\":null}");
    let c2: C5 = C5.from_json(j2);
    let v1: int = match (c1.x) {
        null => -1,
        let n: int => n,
    };
    let v2: int = match (c2.x) {
        null => -1,
        let n: int => n,
    };
    v1 + v2
}

test "optional_primitive_field_from_json" {
    // 42 + (-1) = 41
    assert.equal(optional_primitive_field_from_json_sum(), 41)
}

// map_of_primitive_from_json: C{lookup:map<string,int>}
class C6 { lookup map<string, int> }

function map_of_primitive_from_json_sum() -> int throws baml.json.ParseError | baml.json.DecodeError {
    let j: baml.json.json = baml.json.parse("{\"lookup\":{\"a\":1,\"b\":2,\"c\":3}}");
    let c: C6 = C6.from_json(j);
    c.lookup["a"] + c.lookup["b"] + c.lookup["c"]
}

test "map_of_primitive_from_json" {
    assert.equal(map_of_primitive_from_json_sum(), 6)
}

// top_level_from_json_call_concrete_class
class UserTL { name string  age int }

function top_level_from_json_call_concrete_class_name() -> string throws baml.json.ParseError | baml.json.DecodeError {
    let j: baml.json.json = baml.json.parse("{\"name\":\"Ada\",\"age\":30}");
    let u: UserTL = baml.json.from_json<UserTL>(j);
    u.name
}

test "top_level_from_json_call_concrete_class" {
    assert.equal(top_level_from_json_call_concrete_class_name(), "Ada")
}

// top_level_from_json_call_primitive
function top_level_from_json_call_primitive_val() -> int throws baml.json.ParseError | baml.json.DecodeError {
    let j: baml.json.json = baml.json.parse("42");
    baml.json.from_json<int>(j)
}

test "top_level_from_json_call_primitive" {
    assert.equal(top_level_from_json_call_primitive_val(), 42)
}

// independent_synthesis_only_from_json_defined
class BOnlyFrom {
    y int

    implements baml.FromJson {
        function from_json(j: baml.json.json) -> Self throws baml.json.DecodeError {
            BOnlyFrom { y: 99 }
        }
    }
}

class AOnlyFrom { b BOnlyFrom }

function independent_synthesis_only_from_json_result() -> string throws baml.json.SerializationError | baml.json.ParseError | baml.json.DecodeError {
    let a_in: AOnlyFrom = AOnlyFrom { b: BOnlyFrom { y: 1 } };
    let s: string = baml.json.stringify(a_in.to_json());
    let parsed: AOnlyFrom = AOnlyFrom.from_json(baml.json.parse(s));
    match (parsed.b.y) {
        99 => "override-honored",
        _ => "regression"
    }
}

test "independent_synthesis_only_from_json_defined" {
    assert.equal(independent_synthesis_only_from_json_result(), "override-honored")
}

// independent_synthesis_only_to_json_defined
class BOnlyTo {
    y int

    implements baml.ToJson {
        function to_json(self) -> baml.json.json throws baml.json.SerializationError {
            "[redacted]"
        }
    }
}

class AOnlyTo { b BOnlyTo }

function independent_synthesis_only_to_json_val() -> int throws baml.json.ParseError | baml.json.DecodeError {
    let j: baml.json.json = baml.json.parse("{\"b\":{\"y\":7}}");
    let a: AOnlyTo = AOnlyTo.from_json(j);
    a.b.y
}

test "independent_synthesis_only_to_json_defined" {
    // No user override on from_json side; auto-derive structural decode reads y=7.
    assert.equal(independent_synthesis_only_to_json_val(), 7)
}

// round_trip_with_overrides_both_directions
class SecretBoth {
    data string

    implements baml.ToJson {
        function to_json(self) -> baml.json.json throws baml.json.SerializationError {
            "[redacted]"
        }
    }
    implements baml.FromJson {
        function from_json(j: baml.json.json) -> Self throws baml.json.DecodeError {
            SecretBoth { data: "[reset]" }
        }
    }
}

function round_trip_with_overrides_both_directions_val() -> string throws baml.json.SerializationError | baml.json.ParseError | baml.json.DecodeError {
    let s: SecretBoth = SecretBoth { data: "hunter2" };
    let json_val: baml.json.json = s.to_json();
    let stringified: string = baml.json.stringify(json_val);
    let parsed_json: baml.json.json = baml.json.parse(stringified);
    let s2: SecretBoth = SecretBoth.from_json(parsed_json);
    s2.data
}

test "round_trip_with_overrides_both_directions" {
    assert.equal(round_trip_with_overrides_both_directions_val(), "[reset]")
}

// alias_chain_dispatch_to_json: type Outer = Inner; type Inner = B; b.to_json() → B's override
class BAliasTo {
    y int

    implements baml.ToJson {
        function to_json(self) -> baml.json.json throws baml.json.SerializationError {
            42
        }
    }
}

type InnerTo = BAliasTo
type OuterTo = InnerTo

function alias_chain_dispatch_to_json_val() -> int throws baml.json.SerializationError | baml.json.ParseError {
    let b: OuterTo = BAliasTo { y: 7 };
    let j: baml.json.json = b.to_json();
    match (j) {
        let n: int => n,
        _ => -1,
    }
}

test "alias_chain_dispatch_to_json" {
    assert.equal(alias_chain_dispatch_to_json_val(), 42)
}

// alias_chain_dispatch_from_json: Outer.from_json(j) dispatches B's user from_json override
class BAliasFrom {
    y int

    implements baml.FromJson {
        function from_json(j: baml.json.json) -> Self throws baml.json.DecodeError {
            BAliasFrom { y: 99 }
        }
    }
}

type InnerFrom = BAliasFrom
type OuterFrom = InnerFrom

function alias_chain_dispatch_from_json_val() -> int throws baml.json.ParseError | baml.json.DecodeError {
    let j: baml.json.json = baml.json.parse("{\"y\":7}");
    let b: OuterFrom = OuterFrom.from_json(j);
    b.y
}

test "alias_chain_dispatch_from_json" {
    assert.equal(alias_chain_dispatch_from_json_val(), 99)
}

// from_json_wrapper_body_fallback: class Holder { value unknown }
class Holder { value unknown }

function from_json_wrapper_body_fallback_val() -> int throws baml.json.ParseError | baml.json.DecodeError {
    let j: baml.json.json = baml.json.parse("{\"value\":42}");
    let h: Holder = Holder.from_json(j);
    match (h.value) {
        let n: int => n,
        _ => -1
    }
}

test "from_json_wrapper_body_fallback" {
    assert.equal(from_json_wrapper_body_fallback_val(), 42)
}
