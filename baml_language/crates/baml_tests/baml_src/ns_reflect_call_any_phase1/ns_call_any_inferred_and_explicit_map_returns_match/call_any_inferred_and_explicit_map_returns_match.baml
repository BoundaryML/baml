function make_map() -> map<string, int> throws never {
    let result: map<string, int> = { "alpha": 1, "beta": 2 };
    result
}

function inferred_fallback() -> map<string, int> throws never {
    let result: map<string, int> = { "alpha": -10, "beta": -20 };
    result
}

function explicit_fallback() -> map<string, int> throws never {
    let result: map<string, int> = { "alpha": -100, "beta": -200 };
    result
}

function inferred() -> map<string, int> throws never {
    reflect.call_any(make_map, {}) catch (e) {
        _ => inferred_fallback()
    }
}

function explicit() -> map<string, int> throws never {
    reflect.call_any<map<string, int>, never>(make_map, {}) catch (e) {
        _ => explicit_fallback()
    }
}

function main() -> map<string, int>[] throws never {
    [inferred(), explicit()]
}

test "call_any_inferred_and_explicit_map_returns_match" {
    assert.equal(main(), [{ "alpha": 1, "beta": 2 }, { "alpha": 1, "beta": 2 }])
}
