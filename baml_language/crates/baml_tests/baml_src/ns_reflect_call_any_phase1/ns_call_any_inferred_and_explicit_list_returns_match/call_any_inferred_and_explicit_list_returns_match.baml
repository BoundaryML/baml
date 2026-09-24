function make_list() -> string[] throws never {
    ["alpha", "beta"]
}

function main() -> string throws never {
    let inferred = reflect.call_any(make_list, {}) catch (e) {
        _ => ["inferred error"]
    };
    let explicit = reflect.call_any<string[], never>(make_list, {}) catch (e) {
        _ => ["explicit error"]
    };
    inferred[0] + ":" + inferred[1] + "|" + explicit[0] + ":" + explicit[1]
}

test "call_any_inferred_and_explicit_list_returns_match" {
    assert.equal(main(), "alpha:beta|alpha:beta")
}
