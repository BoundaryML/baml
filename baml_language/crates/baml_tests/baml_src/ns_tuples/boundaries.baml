
function tpj_decode_error(raw: string) -> (string, string)? {
    baml.json.decode<map<string, (int, string)>>(raw) catch (e) {
        baml.json.DecodeError => return (e.message, e.path),
    };
    null
}

test "tuple_json_decode_errors" {
    assert.equal(tpj_decode_error(`{"pair": [1, "a"]}`), null)
    assert.equal(
        tpj_decode_error(`{"pair": [1]}`),
        ("expected an array of 2 elements, got 1", `["pair"]`),
    )
    assert.equal(
        tpj_decode_error(`{"pair": [1, "a", true]}`),
        ("expected an array of 2 elements, got 3", `["pair"]`),
    )
    assert.equal(
        tpj_decode_error(`{"pair": [1, 2]}`),
        ("expected string", `["pair"][1]`),
    )
}

class TpjOverride {
    n: int
    implements baml.FromJson {
        function from_json(j: baml.json.json) -> Self {
            TpjOverride { n: 42 }
        }
    }
}

function tpj_nested_override() -> int {
    let decoded = baml.json.decode<(int, (TpjOverride, bool))>(`[1, [{"n": 1}, true]]`);
    decoded.0 + decoded.1.0.n
}

test "tuple_json_nested_from_json" {
    assert.equal(tpj_nested_override(), 43)
}

function tpj_sap_tuple_first() -> bool {
    let parsed: unknown = baml.sap.parse<(int, int) | int[]>(`[1, 1]`);
    parsed is (int, int)
}

function tpj_sap_list_first() -> bool {
    let parsed: unknown = baml.sap.parse<int[] | (int, int)>(`[1, 1]`);
    reflect.Type.of_value(parsed) == reflect.Type.of<(int, int)>()
}

test "tuple_sap_prefers_tuple" {
    assert.is_true(tpj_sap_tuple_first())
    assert.is_true(tpj_sap_list_first())
}
