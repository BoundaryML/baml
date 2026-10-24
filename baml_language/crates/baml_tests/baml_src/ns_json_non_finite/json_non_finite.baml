// NaN and the infinities in `baml.json`: JSON text has none, so `stringify`
// refuses them, and text that would need one does not parse. A `json` value
// is in memory and holds them, so `from` keeps them and `to<float>` takes
// them. Also the `reason` tag of every value with no JSON form.

class NonFiniteHolder {
    n: float,
}

function stringify_reason(value: unknown) -> string {
    baml.json.stringify(value) catch (e) {
        baml.json.SerializationError => e.reason + " at '" + e.path + "': " + e.message,
    }
}

test "stringify refuses NaN and the infinities (B-1818)" {
    assert.equal(
        stringify_reason(NonFiniteHolder { n: float.nan() }),
        "non_finite at '.n': NaN has no JSON form: a JSON number is finite",
    );
    assert.equal(
        stringify_reason([1.5, float.inf()]),
        "non_finite at '[1]': inf has no JSON form: a JSON number is finite",
    );
    assert.equal(
        stringify_reason({ "low": -float.inf() }),
        "non_finite at '[\"low\"]': -inf has no JSON form: a JSON number is finite",
    );
    assert.equal(
        stringify_reason(NonFiniteHolder { n: float.nan() }),
        stringify_reason(NonFiniteHolder { n: float.nan() }),
    )
}

test "stringify refuses NaN inside a json value" {
    let doc: json = { "items": [1, float.nan()] };
    assert.equal(
        stringify_reason(doc),
        "non_finite at '[\"items\"][1]': NaN has no JSON form: a JSON number is finite",
    )
}

class NonFiniteOverride {
    label: string,
}

implement baml.ToJson for NonFiniteOverride {
    function to_json(self) -> json {
        { "label": self.label, "score": float.nan() }
    }
}

test "stringify refuses NaN an override returns" {
    assert.equal(
        stringify_reason([NonFiniteOverride { label: "a" }]),
        "non_finite at '[0][\"score\"]': NaN has no JSON form: a JSON number is finite",
    )
}

test "from keeps NaN in the json value" {
    let j = baml.json.from(NonFiniteHolder { n: float.nan() });
    let n = baml.json.path<float>(j, ".n");
    assert.is_true(n.is_nan());
    let inf = baml.json.path<float>(baml.json.from([float.inf()]), "[0]");
    assert.is_true(inf.is_infinite());
    let overridden = baml.json.from(NonFiniteOverride { label: "a" });
    assert.is_true(baml.json.path<float>(overridden, ".score").is_nan())
}

test "to takes NaN and the infinities for a float" {
    let holder = baml.json.to<NonFiniteHolder>({ "n": float.nan() });
    assert.is_true(holder.n.is_nan());
    assert.is_true(baml.json.to<float>(float.inf()).is_infinite());
    let either = baml.json.to<float | int>(float.nan());
    assert.is_true(either is float)
}

test "a json target keeps NaN" {
    let copied = baml.json.to<json>({ "n": float.nan() });
    assert.is_true(baml.json.path<float>(copied, ".n").is_nan())
}

function parse_message(text: string) -> string {
    {
        let _ = baml.json.parse(text);
        "parsed"
    } catch (e) {
        baml.json.ParseError => e.message,
    }
}

function decode_message<T>(text: string) -> string {
    {
        let _ = baml.json.decode<T>(text);
        "decoded"
    } catch (e) {
        baml.json.ParseError => "ParseError: " + e.message,
        baml.json.DecodeError => "DecodeError: " + e.message,
    }
}

test "a number past float does not parse (B-1818)" {
    assert.equal(parse_message("1e400"), "number out of range at line 1 column 5");
    assert.equal(parse_message("[1, -2.5e999]"), "number out of range at line 1 column 12");
    assert.equal(
        decode_message<float>("1e400"),
        "ParseError: number out of range at line 1 column 5",
    );
    assert.equal(
        decode_message<NonFiniteHolder>("{\"n\": 1e400}"),
        "ParseError: number out of range at line 1 column 11",
    );
    // A key `decode` skips is still checked.
    assert.equal(
        decode_message<NonFiniteHolder>("{\"n\": 1, \"x\": 1e400}"),
        "ParseError: number out of range at line 1 column 19",
    );
    assert.equal(
        decode_message<json>("1e400"),
        "ParseError: number out of range at line 1 column 5",
    )
}

test "a tiny number is zero and a huge integer is a bigint" {
    assert.equal(baml.json.decode<float>("1e-400"), 0.0);
    let big = baml.json.parse("1" + "0".repeat(400));
    assert.equal(big is bigint, true)
}

// ─── The reason tags ─────────────────────────────────────────────────────────

function from_reason(value: unknown) -> string {
    let j = baml.json.from(value) catch (e) {
        baml.json.SerializationError => {
            return e.reason;
        },
    };
    "ok"
}

function returns_one() -> int {
    1
}

test "each value with no JSON form names its reason" {
    let bytes = baml.Uint8Array.from_hex("00ff");
    assert.equal(from_reason(bytes), "uint8array");
    assert.equal(stringify_reason([bytes]).split(" ").at(0), "uint8array");
    assert.equal(from_reason(returns_one), "function");
    assert.equal(
        from_reason((x: int) -> int {
            x + 1
        }),
        "function",
    );
    assert.equal(from_reason({ "f": returns_one }), "function");
    let task = spawn { 1 };
    assert.equal(from_reason(task), "future");
    let _ = await task;
    let keyed: map<int, string> = {};
    keyed[1] = "one";
    assert.equal(from_reason(keyed), "map_key")
}
