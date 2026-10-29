// A `@@rust_type` class's state is the runtime's own: no decoder, parser or
// reflection builds one from fields, and a generic one is still a member of
// its own type.

function decoded_regex() -> string {
    let built = baml.json.decode<baml.regex.Regex>("{}") catch (e) {
        _ => {
            return "refused";
        },
    };
    "built"
}

function yaml_limit() -> string {
    let built = baml.yaml.decode<baml.spawn.Limit>("{}") catch (e) {
        _ => {
            return "refused";
        },
    };
    "built"
}

function json_to_limit() -> string {
    let built = baml.json.to<baml.spawn.Limit>({}) catch (e) {
        _ => {
            return "refused";
        },
    };
    "built"
}

function parsed_hasher() -> string {
    let built = baml.sap.parse<baml.hash.DefaultHasher>("{}") catch (e) {
        _ => {
            return "refused";
        },
    };
    "built"
}

function coerced_regex() -> string {
    let built = baml.sap.coerce<baml.regex.Regex>({}) catch (e) {
        _ => {
            return "refused";
        },
    };
    "built"
}

function csv_regex() -> string {
    let built = baml.csv.decode<baml.regex.Regex>("a\n1\n") catch (e) {
        _ => {
            return "refused";
        },
    };
    "built"
}

class HoldsRegex {
    r: baml.regex.Regex,
}

function decoded_holder() -> string {
    let built = baml.json.decode<HoldsRegex>("{\"r\": {}}") catch (e) {
        _ => {
            return "refused";
        },
    };
    "built"
}

test "decoders refuse a class only the runtime builds" {
    assert.equal(decoded_regex(), "refused");
    assert.equal(yaml_limit(), "refused");
    assert.equal(json_to_limit(), "refused");
    assert.equal(decoded_holder(), "refused");
    assert.equal(csv_regex(), "refused");
}

test "parsers refuse a class only the runtime builds" {
    assert.equal(parsed_hasher(), "refused");
    assert.equal(coerced_regex(), "refused");
}

test "a parse picks the arm JSON can build" {
    let parsed: baml.regex.Regex | string = baml.sap.parse<baml.regex.Regex | string>("{}");
    assert.equal(parsed is string, true)
}

function schema_of_holder() -> string {
    let schema = baml.json.schema(reflect.Type.of<HoldsRegex>()) catch (e) {
        _ => {
            return "refused";
        },
    };
    "built"
}

test "a schema refuses a class only the runtime builds" {
    assert.equal(schema_of_holder(), "refused")
}

function cache_arm(u: unknown) -> string {
    match (u) {
        let c: baml.sap._ParseCache<string> => "string cache",
        let c: baml.sap._ParseCache<int> => "int cache",
        _ => "other",
    }
}

test "a generic rust_type instance is a member of its own type" {
    let cache = baml.sap._new_parse_cache<int>();
    assert.equal(cache_arm(cache), "int cache");
    assert.equal(cache_arm(baml.deep_copy(cache)), "int cache");
    assert.equal(cache_arm(1), "other");
}
