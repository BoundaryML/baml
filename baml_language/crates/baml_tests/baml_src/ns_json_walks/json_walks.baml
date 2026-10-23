// How `baml.json` walks a value: one that contains itself, one nested deeper
// than any recursion could follow, the keys `decode<T>` skips, `baml.ToJson`
// and `baml.FromJson` overrides inside lists, maps, and classes, and where a
// syntax error is.

// ─── A value that contains itself ─────────────────────────────────────────────

class Link {
    name: string,
    next: Link?,
}

function cyclic_map() -> map<string, json> {
    let m: map<string, json> = { "a": 1 };
    m["self"] = m;
    m
}

function cyclic_link() -> Link {
    let link = Link { name: "a", next: null };
    link.next = link;
    link
}

function stringify_refusal(value: unknown) -> string {
    baml.json.stringify(value) catch (e) {
        baml.json.SerializationError => e.reason + " at " + e.path,
    }
}

function from_refusal(value: unknown) -> string {
    let j = baml.json.from(value) catch (e) {
        baml.json.SerializationError => {
            return e.reason + " at " + e.path;
        },
    };
    baml.json.stringify(j)
}

test "stringify_refuses_a_map_that_contains_itself" {
    assert.equal(stringify_refusal(cyclic_map()), "cycle at [\"self\"]")
}

test "stringify_refuses_an_instance_that_contains_itself" {
    assert.equal(stringify_refusal(cyclic_link()), "cycle at .next")
}

test "from_refuses_a_value_that_contains_itself" {
    assert.equal(from_refusal(cyclic_map()), "cycle at [\"self\"]");
    assert.equal(from_refusal([cyclic_link()]), "cycle at [0].next")
}

test "a_value_reached_twice_without_a_cycle_is_written_twice" {
    let shared: json = [1, 2];
    let twice: json = { "a": shared, "b": shared };
    assert.equal(baml.json.stringify(twice), `{"a":[1,2],"b":[1,2]}`)
}

function to_refusal(j: json) -> string {
    let copy = baml.json.to<json>(j) catch (e) {
        baml.json.DecodeError => {
            return e.message + " at " + e.path;
        },
    };
    baml.json.stringify(copy)
}

test "to_refuses_a_value_that_contains_itself" {
    assert.equal(
        to_refusal(cyclic_map()),
        "cyclic json value: the document contains itself at [\"self\"]",
    )
}

// ─── A value nested very deeply ───────────────────────────────────────────────

function deep_list(depth: int) -> json {
    let j: json = 1;
    for (let i = 0; i < depth; i += 1) {
        j = [j];
    }
    j
}

test "stringify_writes_a_value_nested_100000_deep" {
    assert.equal(baml.json.stringify(deep_list(100000)).length(), 200001)
}

test "from_and_to_copy_a_value_nested_100000_deep" {
    let copy = baml.json.to<json>(baml.json.from(deep_list(100000)));
    assert.equal(baml.json.stringify(copy).length(), 200001)
}

type Nest = Nest[] | int;

function nest_refusal(j: json) -> string {
    let nest = baml.json.to<Nest>(j) catch (e) {
        baml.json.DecodeError => {
            return e.message;
        },
    };
    "decoded"
}

test "to_refuses_a_recursive_alias_nested_past_its_limit" {
    assert.equal(nest_refusal(deep_list(127)), "decoded");
    assert.equal(nest_refusal(deep_list(100000)), "value nests more than 128 levels deep")
}

function parse_refusal(text: string) -> string {
    let j = baml.json.parse(text) catch (e) {
        baml.json.ParseError => {
            return e.message;
        },
    };
    baml.json.stringify(j)
}

test "parse_refuses_nesting_past_its_limit" {
    assert.equal(parse_refusal("[".repeat(127) + "]".repeat(127)).length(), 254);
    assert.equal(
        parse_refusal("[".repeat(128) + "]".repeat(128)),
        "recursion limit exceeded at line 1 column 128",
    )
}

// ─── Keys decode<T> does not read ─────────────────────────────────────────────

class Event {
    id: string,
    n: int,
    note: string?,
}

function decode_event(text: string) -> string {
    let event = baml.json.decode<Event>(text) catch (e) {
        baml.json.ParseError => {
            return "ParseError: " + e.message;
        },
        baml.json.DecodeError => {
            return "DecodeError: " + e.message + " at " + e.path;
        },
    };
    event.id + "/" + event.n.to_string() + "/" + (event.note ?? "none")
}

test "decode_skips_keys_the_class_does_not_declare" {
    assert.equal(
        decode_event(
            `{"extra": {"a": [1, 2.5, {"b": null}], "c": "z"}, "id": "e1", "more": [true, false], "n": 3}`,
        ),
        "e1/3/none",
    )
}

test "decode_reads_a_repeated_key_by_its_last_value" {
    assert.equal(decode_event(`{"id": "e1", "n": "x", "n": 4}`), "e1/4/none")
}

test "decode_checks_the_syntax_of_keys_it_skips" {
    assert.equal(
        decode_event(`{"id": "e1", "n": 3, "extra": [1, 2,]}`),
        "ParseError: trailing comma at line 1 column 37",
    )
}

test "decode_reports_a_syntax_error_before_a_shape_error" {
    assert.equal(
        decode_event(`{"id": 5, "n": 3, "note": `),
        "ParseError: EOF while parsing a value at line 1 column 26",
    )
}

test "decode_reports_the_first_field_in_declaration_order" {
    assert.equal(decode_event(`{"n": "x", "id": 5}`), "DecodeError: expected string at .id")
}

// ─── Overrides inside containers ──────────────────────────────────────────────

// Writes and reads `{"c": degrees}` instead of its field.
class Celsius {
    degrees: float,
    implements baml.ToJson {
        function to_json(self) -> baml.json.json {
            { "c": self.degrees }
        }
    }
    implements baml.FromJson {
        function from_json(j: baml.json.json) -> Self throws baml.json.DecodeError {
            Celsius { degrees: baml.json.to<float>(baml.json.field(j, "c")) }
        }
    }
}

class Station {
    name: string,
    current: Celsius,
    history: Celsius[],
    by_hour: map<string, Celsius>,
    peak: Celsius?,
}

function station() -> Station {
    Station {
        name: "north",
        current: Celsius { degrees: 1.5 },
        history: [Celsius { degrees: 2.5 }, Celsius { degrees: 3.5 }],
        by_hour: { "9": Celsius { degrees: 4.5 } },
        peak: null,
    }
}

const STATION_JSON = `{"name":"north","current":{"c":1.5},"history":[{"c":2.5},{"c":3.5}],"by_hour":{"9":{"c":4.5}},"peak":null}`;

test "stringify_runs_overrides_in_fields_lists_and_maps" {
    assert.equal(baml.json.stringify(station()), STATION_JSON)
}

test "from_runs_overrides_in_fields_lists_and_maps" {
    assert.equal(baml.json.stringify(baml.json.from(station())), STATION_JSON)
}

function degrees(s: Station) -> string {
    let total = s.current.degrees + s.history[0].degrees + s.history[1].degrees;
    total = total + (s.by_hour.get("9")?.degrees ?? 0.0) + (s.peak?.degrees ?? 0.0);
    total.to_string()
}

test "decode_runs_overrides_in_fields_lists_and_maps" {
    assert.equal(degrees(baml.json.decode<Station>(STATION_JSON)), "12.0")
}

test "to_runs_overrides_in_fields_lists_and_maps" {
    assert.equal(degrees(baml.json.to<Station>(baml.json.parse(STATION_JSON))), "12.0")
}

test "decode_runs_an_override_under_an_optional" {
    let s = baml.json.decode<Station>(
        `{"name":"n","current":{"c":1},"history":[],"by_hour":{},"peak":{"c":7.5}}`,
    );
    assert.equal(s.peak?.degrees ?? 0.0, 7.5)
}

// ─── Where a syntax error is ──────────────────────────────────────────────────

test "parse_error_names_the_line_and_column" {
    assert.equal(parse_refusal("{\n  \"a\": tru\n}"), "expected ident at line 3 column 0");
    assert.equal(parse_refusal("[1, 2] x"), "trailing characters at line 1 column 8")
}

// ─── path<T> selectors ────────────────────────────────────────────────────────

const PATH_DOC: json = {
    "name": "Ada",
    "list": [10, 20],
    "n": 5,
    "a.b": "dotted",
    "it's": "quoted",
    "héllo": [1, 2],
    "nested": { "deep": [{ "x": true }] },
};

function path_result(selector: string) -> string {
    let selected = baml.json.path<json>(PATH_DOC, selector) catch (e) {
        baml.json.PathError => {
            return "PathError: " + e.message + " | " + e.selector;
        },
    };
    baml.json.stringify(selected)
}

test "path_selects_with_every_step_form" {
    assert.equal(path_result(".name"), `"Ada"`);
    assert.equal(path_result(".list[1]"), "20");
    assert.equal(path_result("[\"a.b\"]"), `"dotted"`);
    assert.equal(path_result("['it\\'s']"), `"quoted"`);
    assert.equal(path_result(".nested.deep[0].x"), "true");
    assert.equal(path_result("[\"nested\"][\"deep\"][0][\"x\"]"), "true");
    assert.equal(path_result(".héllo[1]"), "2");
    assert.equal(path_result(".list[+1]"), "20");
    assert.equal(path_result(".list[01]"), "20");
    assert.equal(path_result("..name"), `"Ada"`);
    assert.equal(path_result("."), baml.json.stringify(PATH_DOC))
}

test "path_reports_each_kind_of_bad_selector" {
    assert.equal(
        path_result("name"),
        "PathError: selector must start with '.' or '[' (jq style), got 'name' | name",
    );
    assert.equal(path_result(".missing"), "PathError: missing field 'missing' | .missing");
    assert.equal(path_result(".n.x"), "PathError: field access '.x' on a non-object | .n.x");
    assert.equal(
        path_result(".list[5]"),
        "PathError: index [5] out of bounds (length 2) | .list[5]",
    );
    assert.equal(
        path_result(".list[-1]"),
        "PathError: index [-1] out of bounds (length 2) | .list[-1]",
    );
    assert.equal(path_result(".n[0]"), "PathError: index [0] on a non-array | .n[0]");
    assert.equal(path_result(".list["), "PathError: unterminated '[' segment | .list[");
    assert.equal(path_result(".list[0"), "PathError: unterminated '[' segment | .list[0");
    assert.equal(path_result("[\"name"), "PathError: unterminated quoted bracket key | [\"name");
    assert.equal(path_result("['name'x]"), "PathError: unterminated quoted bracket key | ['name'x]");
    assert.equal(
        path_result("[\"name\\"),
        "PathError: unterminated escape in quoted bracket key | [\"name\\",
    );
    assert.equal(path_result(".list[x]"), "PathError: bad bracket token '[x]' | .list[x]");
    assert.equal(path_result(".list[ 1]"), "PathError: bad bracket token '[ 1]' | .list[ 1]");
    assert.equal(
        path_result(".list[4611686018427387904]"),
        "PathError: bad bracket token '[4611686018427387904]' | .list[4611686018427387904]",
    );
    assert.equal(path_result(".list[0]x"), "PathError: expected '.' or '[' at offset 8 | .list[0]x");
    assert.equal(
        path_result(".héllo[0]x"),
        "PathError: expected '.' or '[' at offset 9 | .héllo[0]x",
    )
}

function path_leaf(selector: string) -> string {
    let n = baml.json.path<int>(PATH_DOC, selector) catch (e) {
        baml.json.PathError => {
            return "PathError: " + e.message + " | " + e.selector;
        },
    };
    n.to_string()
}

test "path_decodes_the_leaf_as_t" {
    assert.equal(path_leaf(".n"), "5");
    assert.equal(
        path_leaf(".name"),
        "PathError: leaf does not coerce to the requested type | .name",
    );
    assert.equal(baml.json.path_or<int>(PATH_DOC, ".name", 7), 7);
    assert.equal(baml.json.path_or<int>(PATH_DOC, ".missing", 8), 8);
    assert.equal(baml.json.path_or<int>(PATH_DOC, ".list[0]", 9), 10)
}

// ─── An override behind a cycle, inside a union member ───────────────────────
// Working out whether `ReachX` reaches an override meets `ReachX` again
// through `ReachY`. The answer for `ReachY` must not be kept as `false` while
// `ReachX` is still open: `ReachX` turns out `true` through `z`, so `ReachY`
// does too, and the `z` nested in `y.x` must still go through `from_json`.

class ReachZ {
    v: int,

    implements baml.FromJson {
        function from_json(value: baml.json.json) -> Self throws never {
            ReachZ { v: 42 }
        }
    }
}

class ReachX {
    y: ReachY | string,
    z: ReachZ,
}

class ReachY {
    x: ReachX?,
}

function nested_reach_z(decoded: ReachX) -> int {
    match (decoded.y) {
        let y: ReachY => {
            match (y.x) {
                let x: ReachX => x.z.v,
                null => -1,
            }
        },
        string => -2,
    }
}

test "an override behind a cycle is found inside a union member" {
    let text = `{"y": {"x": {"y": "leaf", "z": {"v": 1}}}, "z": {"v": 1}}`;
    assert.equal(nested_reach_z(baml.json.decode<ReachX>(text)), 42);
    assert.equal(nested_reach_z(baml.json.to<ReachX>(baml.json.parse(text))), 42)
}

// ─── A long chain of recursive classes ───────────────────────────────────────
// Each class has two optional children and an optional link back to the
// first. Working out whether the first reaches an override must visit each
// class once: a walk that redid every open answer per field would take
// about 2^30 steps here.

class ChainLink0 {
    a: ChainLink1?,
    b: ChainLink1?,
    back: ChainLink0?,
}

class ChainLink1 {
    a: ChainLink2?,
    b: ChainLink2?,
    back: ChainLink0?,
}

class ChainLink2 {
    a: ChainLink3?,
    b: ChainLink3?,
    back: ChainLink0?,
}

class ChainLink3 {
    a: ChainLink4?,
    b: ChainLink4?,
    back: ChainLink0?,
}

class ChainLink4 {
    a: ChainLink5?,
    b: ChainLink5?,
    back: ChainLink0?,
}

class ChainLink5 {
    a: ChainLink6?,
    b: ChainLink6?,
    back: ChainLink0?,
}

class ChainLink6 {
    a: ChainLink7?,
    b: ChainLink7?,
    back: ChainLink0?,
}

class ChainLink7 {
    a: ChainLink8?,
    b: ChainLink8?,
    back: ChainLink0?,
}

class ChainLink8 {
    a: ChainLink9?,
    b: ChainLink9?,
    back: ChainLink0?,
}

class ChainLink9 {
    a: ChainLink10?,
    b: ChainLink10?,
    back: ChainLink0?,
}

class ChainLink10 {
    a: ChainLink11?,
    b: ChainLink11?,
    back: ChainLink0?,
}

class ChainLink11 {
    a: ChainLink12?,
    b: ChainLink12?,
    back: ChainLink0?,
}

class ChainLink12 {
    a: ChainLink13?,
    b: ChainLink13?,
    back: ChainLink0?,
}

class ChainLink13 {
    a: ChainLink14?,
    b: ChainLink14?,
    back: ChainLink0?,
}

class ChainLink14 {
    a: ChainLink15?,
    b: ChainLink15?,
    back: ChainLink0?,
}

class ChainLink15 {
    a: ChainLink16?,
    b: ChainLink16?,
    back: ChainLink0?,
}

class ChainLink16 {
    a: ChainLink17?,
    b: ChainLink17?,
    back: ChainLink0?,
}

class ChainLink17 {
    a: ChainLink18?,
    b: ChainLink18?,
    back: ChainLink0?,
}

class ChainLink18 {
    a: ChainLink19?,
    b: ChainLink19?,
    back: ChainLink0?,
}

class ChainLink19 {
    a: ChainLink20?,
    b: ChainLink20?,
    back: ChainLink0?,
}

class ChainLink20 {
    a: ChainLink21?,
    b: ChainLink21?,
    back: ChainLink0?,
}

class ChainLink21 {
    a: ChainLink22?,
    b: ChainLink22?,
    back: ChainLink0?,
}

class ChainLink22 {
    a: ChainLink23?,
    b: ChainLink23?,
    back: ChainLink0?,
}

class ChainLink23 {
    a: ChainLink24?,
    b: ChainLink24?,
    back: ChainLink0?,
}

class ChainLink24 {
    a: ChainLink25?,
    b: ChainLink25?,
    back: ChainLink0?,
}

class ChainLink25 {
    a: ChainLink26?,
    b: ChainLink26?,
    back: ChainLink0?,
}

class ChainLink26 {
    a: ChainLink27?,
    b: ChainLink27?,
    back: ChainLink0?,
}

class ChainLink27 {
    a: ChainLink28?,
    b: ChainLink28?,
    back: ChainLink0?,
}

class ChainLink28 {
    a: ChainLink29?,
    b: ChainLink29?,
    back: ChainLink0?,
}

class ChainLink29 {
    back: ChainLink0?,
}

test "a long chain of recursive classes decodes in one pass over its types" {
    let decoded = baml.json.decode<ChainLink0>(`{"a": {"b": {}}, "back": {}}`);
    assert.equal(decoded.b, null)
}
