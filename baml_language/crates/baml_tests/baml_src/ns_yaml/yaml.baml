// Runtime tests for the `baml.yaml` standard-library namespace.
//
// YAML is intentionally projected into the existing `baml.json.json` algebra.
// These tests assert accepted JSON-compatible shapes and the main rejection
// boundaries from the stdlib contract.

class YamlUser {
    name string
    age int
}

class YamlWrappedName {
    name string

    implements baml.FromJson {
        function from_json(j: baml.json.json) -> Self throws baml.json.DecodeError {
            YamlWrappedName { name: "wrapped:" + baml.json.from_json<string>(baml.json.field(j, "name")) }
        }
    }
}

test "yaml_parse_scalars" {
    assert.equal(baml.json.stringify(baml.yaml.parse("null")), "null")
    assert.equal(baml.json.stringify(baml.yaml.parse("true")), "true")
    assert.equal(baml.json.stringify(baml.yaml.parse("42")), "42")
    assert.equal(baml.json.stringify(baml.yaml.parse("1.5")), "1.5")
    assert.equal(baml.json.stringify(baml.yaml.parse("\"hello\"")), "\"hello\"")
}

test "yaml_parse_sequence" {
    assert.equal(
        baml.json.stringify(baml.yaml.parse("- one\n- two\n- 3\n")),
        "[\"one\",\"two\",3]",
    )
}

test "yaml_parse_nested_mapping" {
    assert.equal(
        baml.json.stringify(baml.yaml.parse("user:\n  name: Ada\n  age: 30\nok: true\n")),
        "{\"user\":{\"name\":\"Ada\",\"age\":30},\"ok\":true}",
    )
}

test "yaml_parse_empty_input_is_null" {
    assert.equal(baml.json.stringify(baml.yaml.parse("")), "null")
}

test "yaml_parse_resolves_anchors_and_aliases" {
    assert.equal(
        baml.json.stringify(baml.yaml.parse("a: &x 1\nb: *x\n")),
        "{\"a\":1,\"b\":1}",
    )
}

test "yaml_deserialize_decodes_class" {
    let user: YamlUser = baml.yaml.deserialize<YamlUser>("name: Ada\nage: 30\n");
    assert.equal(user.name, "Ada")
    assert.equal(user.age, 30)
}

test "yaml_deserialize_honors_from_json_override" {
    let user: YamlWrappedName = baml.yaml.deserialize<YamlWrappedName>("name: Ada\n");
    assert.equal(user.name, "wrapped:Ada")
}

test "yaml_deserialize_decode_failure" {
    {
        let user: YamlUser = baml.yaml.deserialize<YamlUser>("name: Ada\n");
        baml.sys.panic("expected missing age to throw")
    } catch (e) {
        baml.json.DecodeError => null
    }
}

test "yaml_parse_malformed_throws" {
    {
        baml.yaml.parse("[");
        baml.sys.panic("expected malformed YAML to throw")
    } catch (e) {
        baml.yaml.ParseError => null
    }
}

test "yaml_parse_non_string_map_key_throws" {
    {
        baml.yaml.parse("1: one\n");
        baml.sys.panic("expected non-string YAML key to throw")
    } catch (e) {
        baml.yaml.ParseError => null
    }
}

test "yaml_parse_integer_overflow_throws" {
    {
        baml.yaml.parse("9223372036854775808");
        baml.sys.panic("expected out-of-range YAML integer to throw")
    } catch (e) {
        baml.yaml.ParseError => null
    }
}

test "yaml_parse_non_finite_float_throws" {
    {
        baml.yaml.parse(".inf");
        baml.sys.panic("expected non-finite YAML float to throw")
    } catch (e) {
        baml.yaml.ParseError => null
    }
    {
        baml.yaml.parse(".nan");
        baml.sys.panic("expected NaN YAML float to throw")
    } catch (e) {
        baml.yaml.ParseError => null
    }
}

test "yaml_parse_duplicate_key_throws" {
    {
        baml.yaml.parse("name: Ada\nname: Grace\n");
        baml.sys.panic("expected duplicate YAML key to throw")
    } catch (e) {
        baml.yaml.ParseError => null
    }
}

test "yaml_parse_multi_document_throws" {
    {
        baml.yaml.parse("a: 1\n---\nb: 2\n");
        baml.sys.panic("expected multi-document YAML to throw")
    } catch (e) {
        baml.yaml.ParseError => null
    }
}

test "yaml_parse_tag_throws" {
    {
        baml.yaml.parse("!custom value");
        baml.sys.panic("expected tagged YAML to throw")
    } catch (e) {
        baml.yaml.ParseError => null
    }
}
