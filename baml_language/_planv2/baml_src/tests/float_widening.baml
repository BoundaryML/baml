// The reflection call boundary widens integral JSON numbers into float
// parameters (JSON Schema `number` accepts integers; models emit 150 for
// 150.0). The widening lives in reflect.call_any itself, not in ai code.

test "an int argument widens into a float tool parameter" {
    let args: map<string, unknown> = {};
    let _ = args.set("city", "Kyoto");
    let _ = args.set("max_nightly_usd", 150);
    let out = root.ai.tool(search_hotels).call(args);
    assert.is_true(out.includes("Sakura Inn"))
}
