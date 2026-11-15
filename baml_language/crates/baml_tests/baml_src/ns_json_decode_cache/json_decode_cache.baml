// Each VM thread keeps what `baml.json` decoding works out about a type (its
// class plans, the `from_json` a rule provides, whether an override is in
// reach) from one call to the next. These tests warm that cache and check that
// decoding still follows what each call asks: both namings, a collection that
// moves the runtime class a warm answer names, a decode nested inside a
// provided `from_json`, and a failed union whose message is worked out once.
// (A session republish that adds a `FromJson` impl to a warm type is covered
// by `ns_impl_resolution_cache`.)

// ─── each naming keeps its own answers ───────────────────────────────────────

class JdcPoint {
    x: int @alias("ex"),
    y: int,
}

test "warm_decodes_keep_each_naming_apart" {
    let aliased = baml.json.Naming.Aliased;
    let total = 0;
    for (let i = 0; i < 50; i += 1) {
        total += baml.json.decode<JdcPoint>("{\"x\": 1, \"y\": 2}").x;
        total += baml.json.decode<JdcPoint>("{\"ex\": 10, \"y\": 2}", naming = aliased).x;
        total += baml.json.to<JdcPoint>({ "x": 100, "y": 2 }).x;
    }
    assert.equal(total, 50 * 111);
    // The aliased key is unknown under declared names, and the reverse.
    let declared_misses = baml.json.decode<JdcPoint>("{\"ex\": 1, \"y\": 2}") catch (e) {
        baml.json.DecodeError => {
            e.message
        }
    };
    assert.equal(declared_misses, "missing required field `x`");
    let aliased_misses = baml.json.decode<JdcPoint>("{\"x\": 1, \"y\": 2}", naming = aliased) catch (e) {
        baml.json.DecodeError => {
            e.message
        }
    };
    assert.equal(aliased_misses, "missing required field `ex`")
}

// ─── a collection drops warm answers about runtime classes ───────────────────

// Decode `n` badges as a freshly minted class; each round trips to the same
// text, and the ranks sum to 0 + 1 + ... + (n - 1).
function jdc_decode_minted(label: string, n: int) -> int {
    let minted = reflect.Class.new("Badge", { "label": string.type(), "rank": int.type() });
    let type Badge = minted;
    let total = 0;
    for (let i = 0; i < n; i += 1) {
        let text = "{\"label\":\"" + label + "\",\"rank\":" + i.to_string() + "}";
        let badge: Badge = baml.json.decode<Badge>(text);
        if (baml.json.stringify(badge) != text) {
            throw "decoded " + baml.json.stringify(badge) + " from " + text;
        }
        total += baml.json.path<int>(baml.json.from(badge), ".rank");
    }
    total
}

function jdc_churn_and_collect() -> null {
    for (let round = 0; round < 3; round += 1) {
        let garbage: string[][] = [];
        for (let i = 0; i < 2000; i += 1) {
            garbage.push(["x", "y", "z"]);
        }
        baml.sys.collect_garbage();
    }
    null
}

test "warm_decodes_of_runtime_classes_survive_collection" {
    assert.equal(jdc_decode_minted("gold", 20), 190);
    jdc_churn_and_collect();
    // A class minted after the collection, with the same name, decodes by its
    // own fields.
    assert.equal(jdc_decode_minted("tin", 20), 190);
    let other = reflect.Class.new("Badge", { "label": int.type() });
    let type Other = other;
    let refused = baml.json.decode<Other>("{\"label\": \"tin\"}") catch (e) {
        baml.json.DecodeError => {
            e.path + ": " + e.message
        }
    };
    assert.equal(refused, ".label: expected integer")
}

// One minted class decoded warm, then the heap churned and collected while the
// class stays in use (the collection moves it), then decoded again: the warm
// answers were dropped and are worked out anew at its new address.
function jdc_decode_across_collection(label: string) -> string {
    let kept = reflect.Class.new("Badge", { "label": string.type(), "rank": int.type() });
    let type Kept = kept;
    let before: Kept[] = [];
    for (let i = 0; i < 10; i += 1) {
        before.push(
            baml.json.decode<Kept>("{\"label\":\"" + label + "\",\"rank\":" + i.to_string() + "}"),
        );
    }
    jdc_churn_and_collect();
    let after: Kept = baml.json.decode<Kept>("{\"rank\":10,\"label\":\"" + label + "\"}");
    baml.json.stringify(before[9]) + " " + baml.json.stringify(after)
}

test "a_runtime_class_kept_across_a_collection_decodes_after_it" {
    assert.equal(
        jdc_decode_across_collection("gold"),
        "{\"label\":\"gold\",\"rank\":9} {\"label\":\"gold\",\"rank\":10}",
    )
}

// ─── a decode inside a provided `from_json` ──────────────────────────────────

class JdcCelsius {
    degrees: float,
    implements baml.FromJson {
        function from_json(j: baml.json.json) -> Self throws baml.json.DecodeError {
            // A decode of its own while the outer decode waits for this one.
            let reading = baml.json.to<JdcReading>(j);
            JdcCelsius { degrees: reading.value * 2.0 }
        }
    }
}

class JdcReading {
    value: float,
}

class JdcStation {
    name: string,
    temps: JdcCelsius[],
}

test "decodes_nested_in_a_from_json_share_warm_answers" {
    let total = 0.0;
    for (let i = 0; i < 30; i += 1) {
        let station = baml.json.decode<JdcStation>(
            "{\"name\": \"north\", \"temps\": [{\"value\": 1.5}, {\"value\": 2.0}]}",
        );
        for (let t in station.temps) {
            total += t.degrees;
        }
    }
    assert.equal(total, 30.0 * 7.0)
}

// ─── a failed union names its impossible members every time ──────────────────

class JdcLeft {
    left: int,
}

class JdcRight {
    right: int,
}

function jdc_union_message(text: string) -> string {
    let decoded = baml.json.decode<JdcLeft | JdcRight | uint8array>(text) catch (e) {
        baml.json.DecodeError => {
            return e.message;
        }
    };
    "decoded"
}

test "a_warm_failing_union_reports_the_same_message" {
    let first = jdc_union_message("{\"left\": \"x\"}");
    for (let i = 0; i < 20; i += 1) {
        assert.equal(jdc_union_message("{\"left\": \"x\"}"), first)
    }
    assert.is_true(first.includes("`uint8array` is not possible here"));
    assert.equal(jdc_union_message("{\"right\": 2}"), "decoded")
}
