// The baml.json conversions: `from`, `stringify`, `to<T>`, and `decode<T>`,
// with their `naming`, `indent`, and `omit_nulls` options.

enum ApiTone {
    Calm @alias("calm")
    Loud
    Hidden @skip
}

class ApiNamed {
    title: string @alias("headline"),
    tone: ApiTone,
    note: string @skip,
}

class ApiBox {
    items: ApiNamed[],
}

function named() -> ApiNamed {
    ApiNamed { title: "Hi", tone: ApiTone.Calm, note: "secret" }
}

// ─── naming ───────────────────────────────────────────────────────────────────

test "from writes declared names by default" {
    assert.equal(baml.json.stringify(baml.json.from(named())), `{"title":"Hi","tone":"Calm","note":"secret"}`)
}

test "from with aliased names uses aliases and leaves out skip fields" {
    let j = baml.json.from(named(), naming = baml.json.Naming.Aliased);
    assert.equal(baml.json.stringify(j), `{"headline":"Hi","tone":"calm"}`)
}

test "stringify writes declared names by default" {
    assert.equal(baml.json.stringify(named()), `{"title":"Hi","tone":"Calm","note":"secret"}`)
}

test "stringify with aliased names uses aliases and leaves out skip fields" {
    assert.equal(
        baml.json.stringify(named(), naming = baml.json.Naming.Aliased),
        `{"headline":"Hi","tone":"calm"}`,
    )
}

function stringify_skipped_variant() -> string {
    baml.json.stringify(ApiTone.Hidden, naming = baml.json.Naming.Aliased) catch (e) {
        let err: baml.json.SerializationError => err.message,
    }
}

test "stringify with aliased names rejects a skip enum variant" {
    assert.contains(stringify_skipped_variant(), "is `@skip`")
    assert.equal(baml.json.stringify(ApiTone.Hidden), `"Hidden"`)
}

function to_named(j: json, naming: baml.json.Naming) -> string {
    let n = baml.json.to<ApiNamed>(j, naming = naming);
    `${n.title}|${n.tone}|[${n.note}]`
}

test "to reads declared names by default" {
    assert.equal(to_named({ "title": "Hi", "tone": "Hidden", "note": "kept" }, baml.json.Naming.Declared), "Hi|Hidden|[kept]")
}

test "to with aliased names reads aliases and holds empty values for skip fields" {
    assert.equal(to_named({ "headline": "Hi", "tone": "calm", "note": "ignored" }, baml.json.Naming.Aliased), "Hi|Calm|[]")
}

function decode_named(text: string) -> string {
    let n = baml.json.decode<ApiNamed>(text, naming = baml.json.Naming.Aliased);
    `${n.title}|${n.tone}|[${n.note}]`
}

test "decode with aliased names reads aliases and holds empty values for skip fields" {
    assert.equal(decode_named(`{"headline": "Hi", "tone": "calm"}`), "Hi|Calm|[]")
}

function to_error_path() -> string {
    let j: json = { "headline": 5, "tone": "calm" };
    {
        baml.json.to<ApiNamed>(j, naming = baml.json.Naming.Aliased);
        "decoded"
    } catch (e) {
        let err: baml.json.DecodeError => err.path,
    }
}

function decode_error_path() -> string {
    {
        baml.json.decode<ApiBox>(`{"items": [{"headline": "Hi", "tone": "Loud"}, {"headline": 5, "tone": "calm"}]}`, naming = baml.json.Naming.Aliased);
        "decoded"
    } catch (e) {
        let err: baml.json.DecodeError => err.path,
        let err: baml.json.ParseError => err.message,
    }
}

test "a decode error path names the JSON key" {
    assert.equal(to_error_path(), ".headline")
    assert.equal(decode_error_path(), ".items[1].headline")
}

function round_trip_aliased() -> string {
    let text = baml.json.stringify(named(), naming = baml.json.Naming.Aliased);
    let back = baml.json.decode<ApiNamed>(text, naming = baml.json.Naming.Aliased);
    `${back.title}|${back.tone}|[${back.note}]`
}

test "aliased text round-trips through stringify and decode" {
    assert.equal(round_trip_aliased(), "Hi|Calm|[]")
}

// ─── indent ───────────────────────────────────────────────────────────────────

test "stringify is compact by default" {
    assert.equal(baml.json.stringify({ "a": [1, 2], "b": null }), `{"a":[1,2],"b":null}`)
    assert.equal(baml.json.stringify({ "a": 1 }, indent = 0), `{"a":1}`)
    assert.equal(baml.json.stringify({ "a": 1 }, indent = ""), `{"a":1}`)
}

test "stringify indents by a count of spaces" {
    assert.equal(baml.json.stringify({ "a": [1, 2] }, indent = 2), "{\n  \"a\": [\n    1,\n    2\n  ]\n}")
    assert.equal(baml.json.stringify([1], indent = 4), "[\n    1\n]")
}

test "stringify indents by a string" {
    assert.equal(baml.json.stringify({ "a": 1 }, indent = "\t"), "{\n\t\"a\": 1\n}")
}

test "stringify keeps empty containers on one line when indenting" {
    assert.equal(baml.json.stringify({ "a": [], "b": {} }, indent = 2), "{\n  \"a\": [],\n  \"b\": {}\n}")
}

function bad_indent(indent: int | string) -> string {
    baml.json.stringify([1], indent = indent) catch (e) {
        let err: baml.json.SerializationError => err.reason,
    }
}

test "stringify rejects a negative indent and an indent that is not whitespace" {
    assert.equal(bad_indent(-1), "indent")
    assert.equal(bad_indent("ab"), "indent")
    assert.equal(bad_indent(" x"), "indent")
}

// ─── omit_nulls ───────────────────────────────────────────────────────────────

class ApiOptional {
    name: string,
    nickname: string?,
    tags: map<string, string?>,
    items: (int | null)[],
    data: json,
}

function optional_value() -> ApiOptional {
    ApiOptional {
        name: "Ada",
        nickname: null,
        tags: { "a": null },
        items: [1, null],
        data: { "x": null, "y": [null] },
    }
}

test "omit_nulls is off by default" {
    assert.equal(
        baml.json.stringify(optional_value()),
        `{"name":"Ada","nickname":null,"tags":{"a":null},"items":[1,null],"data":{"x":null,"y":[null]}}`,
    )
}

test "stringify with omit_nulls leaves out null class fields only" {
    assert.equal(
        baml.json.stringify(optional_value(), omit_nulls = true),
        `{"name":"Ada","tags":{"a":null},"items":[1,null],"data":{"x":null,"y":[null]}}`,
    )
}

test "from with omit_nulls leaves out null class fields at any depth" {
    let j = baml.json.from({ "row": [optional_value()] }, omit_nulls = true);
    assert.equal(
        baml.json.stringify(j),
        `{"row":[{"name":"Ada","tags":{"a":null},"items":[1,null],"data":{"x":null,"y":[null]}}]}`,
    )
}

// ─── baml.ToJson overrides do not see the options ─────────────────────────────

class ApiCustom {
    inner_value: int @alias("innerValue"),

    implements baml.ToJson {
        function to_json(self) -> json throws baml.json.SerializationError {
            { "inner_value": baml.json.from(self.inner_value), "missing": null }
        }
    }
}

class ApiOuter {
    outer_name: string @alias("outerName"),
    custom: ApiCustom @alias("customValue"),
    gone: string?,
}

test "an override owns its output under aliased names and omit_nulls" {
    let outer = ApiOuter { outer_name: "o", custom: ApiCustom { inner_value: 1 }, gone: null };
    assert.equal(
        baml.json.stringify(outer, naming = baml.json.Naming.Aliased, omit_nulls = true),
        `{"outerName":"o","customValue":{"inner_value":1,"missing":null}}`,
    )
    assert.equal(
        baml.json.stringify(baml.json.from(outer, naming = baml.json.Naming.Aliased, omit_nulls = true)),
        `{"outerName":"o","customValue":{"inner_value":1,"missing":null}}`,
    )
}

class ApiTwoCustom {
    hidden: ApiCustom @skip,
    shown: ApiCustom,
}

test "a skip field holding an override does not shift the overrides after it" {
    let two = ApiTwoCustom { hidden: ApiCustom { inner_value: 1 }, shown: ApiCustom { inner_value: 2 } };
    assert.equal(
        baml.json.stringify(two, naming = baml.json.Naming.Aliased),
        `{"shown":{"inner_value":2,"missing":null}}`,
    )
    assert.equal(
        baml.json.stringify(two),
        `{"hidden":{"inner_value":1,"missing":null},"shown":{"inner_value":2,"missing":null}}`,
    )
}

// ─── unknown values (B-1825) ──────────────────────────────────────────────────

function dump(v: unknown) -> string {
    baml.json.stringify(v)
}

test "stringify writes a value typed unknown" {
    assert.equal(dump(named()), `{"title":"Hi","tone":"Calm","note":"secret"}`)
    assert.equal(dump([1, "a", null]), `[1,"a",null]`)
    assert.equal(dump(ApiCustom { inner_value: 3 }), `{"inner_value":3,"missing":null}`)
}

// ─── decode honors baml.FromJson overrides ────────────────────────────────────

class ApiEvent {
    at: baml.time.Instant,
    label: string @alias("eventLabel"),
}

function decode_event() -> string {
    let event = baml.json.decode<ApiEvent>(
        `{"at": "2024-01-02T03:04:05Z", "eventLabel": "launch"}`,
        naming = baml.json.Naming.Aliased,
    );
    `${event.label}@${baml.json.stringify(event.at)}`
}

test "decode builds a field through its FromJson override" {
    assert.equal(decode_event(), `launch@"2024-01-02T03:04:05Z"`)
}

test "to builds a root through its FromJson override" {
    let at = baml.json.to<baml.time.Instant>("2024-01-02T03:04:05Z");
    assert.equal(baml.json.stringify(at), `"2024-01-02T03:04:05Z"`)
}

// ─── B-1822: `@alias` reaches baml.json through naming ────────────────────────

class ApiOpts {
    enable_escape: bool? @alias("enableEscape"),
}

function escape_flag(text: string, naming: baml.json.Naming) -> bool? {
    baml.json.decode<ApiOpts>(text, naming = naming).enable_escape
}

test "decode reads an alias with aliased names and the declared name by default" {
    assert.equal(escape_flag(`{"enableEscape": true}`, baml.json.Naming.Aliased), true)
    assert.equal(escape_flag(`{"enable_escape": true}`, baml.json.Naming.Declared), true)
}

test "stringify writes an alias with aliased names" {
    assert.equal(
        baml.json.stringify(ApiOpts { enable_escape: true }, naming = baml.json.Naming.Aliased),
        `{"enableEscape":true}`,
    )
}

// ─── B-1816: indent by a tab, as JSON.stringify(v, null, "\t") does ───────────

test "stringify with a tab indent matches JavaScript" {
    let doc: json = { "share": 1.5, "count": 3 };
    assert.equal(baml.json.stringify(doc, indent = "\t"), "{\n\t\"share\": 1.5,\n\t\"count\": 3\n}")
}
