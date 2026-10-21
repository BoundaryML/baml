enum CapturedMapChoice {
    First,
}

class CapturedMapKey {
    id: int
}

function map_keys_echo(value: unknown) -> unknown {
    value
}

function map_keys_capture(scenario: string) -> int {
    let value: unknown = match (scenario) {
        "int" => {
            let entries: map<int, string> = { 7: "int" };
            assert.equal(entries[7], "int");
            entries
        },
        "bool" => {
            let entries: map<bool, string> = { true: "bool" };
            assert.equal(entries[true], "bool");
            entries
        },
        "float" => {
            let entries: map<float, string> = { 1.5: "float" };
            assert.equal(entries[1.5], "float");
            entries
        },
        "null" => {
            let entries: map<int?, string> = { null: "null" };
            assert.equal(entries[null], "null");
            entries
        },
        "bigint" => {
            let entries: map<bigint, string> = {};
            entries.set(9007199254740993n, "bigint");
            assert.equal(entries[9007199254740993n], "bigint");
            entries
        },
        "enum" => {
            let entries: map<CapturedMapChoice, string> = {};
            entries.set(CapturedMapChoice.First, "enum");
            assert.equal(entries[CapturedMapChoice.First], "enum");
            entries
        },
        "array" => {
            let entries: map<int[], string> = {};
            entries.set([1, 2], "array");
            assert.equal(entries[[1, 2]], "array");
            entries
        },
        "class" => {
            let entries: map<CapturedMapKey, string> = {};
            entries.set(CapturedMapKey { id: 7 }, "class");
            assert.equal(entries[CapturedMapKey { id: 7 }], "class");
            entries
        },
        _ => baml.sys.panic("unknown map key scenario"),
    };
    map_keys_echo(value, $trace = trace.span(inputs = true, output = true));
    1
}

test "trace capture accepts scalar and composite map keys" {
    for (let scenario in ["int", "bool", "float", "null", "bigint", "enum", "array", "class"]) {
        assert.equal(map_keys_capture(scenario), 1);
    }
}
