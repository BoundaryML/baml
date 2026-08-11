// The reflection call boundary decodes dynamic arguments value-by-value
// (B-1174): integral JSON numbers widen into float parameters, and JSON
// arrays rebuild element-wise into typed array parameters. Every
// conversion constructs a fresh value — arrays stay invariant; this is a
// boundary decode in reflect.call_any, not a subtyping rule, and it lives
// in the VM, not in ai code.

test "an int argument widens into a float tool parameter" {
    let args: map<string, unknown> = {
        "city": "Kyoto",
        "max_nightly_usd": 150,
    };
    let out = ai.tools.tool(search_hotels).call(args);
    assert.is_true(out.includes("Sakura Inn"))
}

/// Join the given words with spaces.
function join_words(words: string[]) -> string {
    words.join(" ")
}

test "a json string array decodes into a string[] tool parameter" {
    // mirror the wire path: parsed JSON, so the array's element tag is
    // json, not string
    let raw = baml.json.parse(`{"words": ["hello", "world"]}`);
    let args = baml.json.from_json<map<string, unknown>>(raw);
    let out = ai.tools.tool(join_words).call(args);
    assert.contains(out, "hello world")
}

/// Sum the given prices.
function sum_prices(prices: float[]) -> float {
    let total = 0.0;
    for (let p in prices) {
        total += p;
    }
    total
}

test "integral elements widen inside a float[] tool parameter" {
    let raw = baml.json.parse(`{"prices": [150, 200.5]}`);
    let args = baml.json.from_json<map<string, unknown>>(raw);
    let out = ai.tools.tool(sum_prices).call(args);
    assert.contains(out, "350.5")
}

test "a lossy array element still fails validation" {
    // a string element cannot decode into float[]; the call must throw,
    // not truncate or coerce
    let raw = baml.json.parse(`{"prices": [150, "cheap"]}`);
    let args = baml.json.from_json<map<string, unknown>>(raw);
    let failed = (
        ai.tools.tool(sum_prices).call(args) catch_all (e) {
            _ => null,
        }
    )
        == null;
    assert.is_true(failed)
}
