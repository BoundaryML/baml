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
