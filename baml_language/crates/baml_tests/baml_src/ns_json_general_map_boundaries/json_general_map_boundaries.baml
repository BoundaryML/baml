test "encode_rejects_non_string_map" {
    {
        let values: map<int, string> = {};
        values[1] = "one";
        baml.json.to_string(values);
        baml.sys.panic("expected serialization rejection")
    } catch (e) {
        let err: baml.json.SerializationError => {
            assert.contains(err.message, "string keys");
        }
    }
}

test "encode_rejects_empty_non_string_map" {
    {
        let values: map<int, string> = {};
        baml.json.to_string(values);
        baml.sys.panic("expected serialization rejection")
    } catch (e) {
        let err: baml.json.SerializationError => {
            assert.contains(err.message, "string keys");
        }
    }
}

test "decode_rejects_non_string_map" {
    {
        baml.json.from_string<map<int, string>>("{}");
        baml.sys.panic("expected decode rejection")
    } catch (e) {
        let err: baml.json.DecodeError => {
            assert.contains(err.message, "string keys");
        }
    }
}

test "driver_decode_rejects_non_string_map" {
    {
        baml.json.from_json<map<int, string>>(baml.json.parse("{\"1\":\"one\"}"));
        baml.sys.panic("expected decode rejection")
    } catch (e) {
        let err: baml.json.DecodeError => {
            assert.contains(err.message, "string keys");
        }
    }
}

class CustomMapEncoding {
    values: map<int, string>,
    implements baml.ToJson {
        function to_json(self) -> baml.json.json {
            "custom"
        }
    }
}

test "override_precedes_structural_map_rejection" {
    let values: map<int, string> = {};
    values[1] = "one";
    let wrapped = CustomMapEncoding { values: values };
    assert.equal(baml.json.to_string(wrapped), "\"custom\"");
    assert.equal(baml.json.to_string({ "nested": wrapped }), "{\"nested\":\"custom\"}");
}

test "string_maps_preserve_round_trip" {
    let values: map<string, int> = baml.json.from_string<map<string, int>>("{\"one\":1}");
    assert.equal(values["one"], 1);
    assert.equal(baml.json.to_string(values), "{\"one\":1}");
}

type StringKeys = "a" | "b"

type RecursiveMap = int | RecursiveMap[] | map<string, RecursiveMap> | map<int, RecursiveMap>

class RecursiveMapRow {
    data: RecursiveMap,
}

test "recursive_alias_markdown_fallback_rejects_non_string_maps" {
    let values: map<int, RecursiveMap> = {};
    values[1] = 2;
    let children: RecursiveMap[] = [values];
    let nested: map<string, RecursiveMap> = {};
    nested["children"] = children;
    let rendered = baml.csv.to_markdown<RecursiveMapRow>([RecursiveMapRow { data: nested }]);
    assert.contains(rendered, "<unrepresentable>");
}

test "recursive_alias_rejects_non_string_maps" {
    let values: map<int, RecursiveMap> = {};
    values[1] = 2;
    let children: RecursiveMap[] = [values];
    let nested: map<string, RecursiveMap> = {};
    nested["children"] = children;
    {
        baml.json.to_string(nested);
        baml.sys.panic("expected recursive map rejection")
    } catch (e) {
        let err: baml.json.SerializationError => {
            assert.contains(err.message, "string keys");
        }
    }
}

test "recursive_alias_rejects_empty_non_string_maps" {
    let values: map<int, RecursiveMap> = {};
    let nested: RecursiveMap = [values];
    {
        baml.json.to_string(nested);
        baml.sys.panic("expected empty recursive map rejection")
    } catch (e) {
        let err: baml.json.SerializationError => {
            assert.contains(err.message, "string keys");
        }
    }
}

test "recursive_alias_string_maps_preserve_nested_json" {
    let leaf: map<string, RecursiveMap> = {};
    leaf["value"] = 2;
    let children: RecursiveMap[] = [1, leaf];
    let nested: map<string, RecursiveMap> = {};
    nested["children"] = children;
    let encoded = baml.json.to_string(nested);
    assert.equal(encoded, "{\"children\":[1,{\"value\":2}]}");
    let rendered = baml.csv.to_markdown<RecursiveMapRow>([RecursiveMapRow { data: nested }]);
    assert.contains(rendered, encoded);
    let json = baml.json.parse(encoded);
    assert.equal(baml.json.stringify(json), encoded);
    assert.equal(baml.json.stringify(baml.json.field(json, "children")), "[1,{\"value\":2}]");
}

test "encode_accepts_declared_string_subtypes" {
    let values: map<StringKeys, int> = {};
    assert.equal(baml.json.to_string(values), "{}");
    values["a"] = 1;
    values["b"] = 2;
    assert.equal(baml.json.to_string(values), "{\"a\":1,\"b\":2}");
}

test "string_subtype_maps_preserve_round_trip" {
    let literal = baml.json.from_string<map<"a", int>>("{\"a\":1}");
    assert.equal(baml.json.to_string(literal), "{\"a\":1}");
    let union = baml.json.from_string<map<"a" | "b", int>>("{\"a\":1,\"b\":2}");
    assert.equal(baml.json.to_string(union), "{\"a\":1,\"b\":2}");
    let alias = baml.json.from_json<map<StringKeys, int>>(baml.json.parse("{\"a\":1,\"b\":2}"));
    assert.equal(alias["b"], 2);
    assert.equal(baml.json.to_string(alias), "{\"a\":1,\"b\":2}");
}

test "string_subtype_decode_validates_keys" {
    {
        baml.json.from_string<map<StringKeys, int>>("{\"c\":1}");
        baml.sys.panic("expected key rejection")
    } catch (e) {
        let err: baml.json.DecodeError => {
            assert.contains(err.message, "key type");
        }
    }
    {
        baml.json.from_json<map<"a", int>>(baml.json.parse("{\"b\":1}"));
        baml.sys.panic("expected key rejection")
    } catch (e) {
        let err: baml.json.DecodeError => {
            assert.contains(err.message, "key type");
        }
    }
}
