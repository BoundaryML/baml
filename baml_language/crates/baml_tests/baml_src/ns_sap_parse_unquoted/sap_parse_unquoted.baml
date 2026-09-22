// Regression tests for #4790: baml.sap.parse dropped every field after an
// unquoted scalar value in compact (whitespace-free) JSON-like input, e.g.
// `{routes:[{id:"001",schema_id:null,reason:"..."}]}` — the unquoted `null`
// swallowed the `,` and everything up to the closing `}`, so the entry lost
// its required `reason` field and was silently dropped from the array.
//
// User types are prefixed `SapUnquoted` to avoid corpus-level collisions.

class SapUnquotedPageRoute {
    id string
    schema_id string?
    reason string
}

class SapUnquotedPageRoutes {
    routes SapUnquotedPageRoute[]
}

function sap_unquoted_parse_compact_routes() -> int {
    let result = baml.sap.parse<SapUnquotedPageRoutes>(`{routes:[{id:"001",schema_id:null,reason:"amount -1.617,98"}]}`);
    result.routes.length()
}

test "sap_parse_keeps_unquoted_key_array_entry" {
    assert.equal(sap_unquoted_parse_compact_routes(), 1)
}

function sap_unquoted_parse_compact_routes_two_entries() -> int {
    let result = baml.sap.parse<SapUnquotedPageRoutes>(
        `{routes:[{id:"001",schema_id:null,reason:"first"},{id:"002",schema_id:null,reason:"second"}]}`,
    );
    result.routes.length()
}

test "sap_parse_keeps_both_unquoted_key_array_entries" {
    assert.equal(sap_unquoted_parse_compact_routes_two_entries(), 2)
}

// Control: the fully quoted document takes the strict serde path and always worked.
function sap_unquoted_parse_quoted_key_routes() -> int {
    let result = baml.sap.parse<SapUnquotedPageRoutes>(
        `{"routes":[{"id":"001","schema_id":null,"reason":"amount -1.617,98"}]}`,
    );
    result.routes.length()
}

test "sap_parse_keeps_quoted_key_array_entry" {
    assert.equal(sap_unquoted_parse_quoted_key_routes(), 1)
}

// The parsed entry must keep its field values, not just the array length.
function sap_unquoted_parse_compact_reason() -> string {
    let result = baml.sap.parse<SapUnquotedPageRoutes>(`{routes:[{id:"001",schema_id:null,reason:"amount -1.617,98"}]}`);
    match (result.routes.at(0)) {
        null => "missing",
        let r: SapUnquotedPageRoute => r.reason,
    }
}

test "sap_parse_keeps_unquoted_entry_fields" {
    assert.equal(sap_unquoted_parse_compact_reason(), "amount -1.617,98")
}
