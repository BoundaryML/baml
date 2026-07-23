// Native runtime coverage for object/map property shorthand.
//
// A bare property name is equivalent to `name: name`; explicit properties can
// be mixed with shorthand in any position.

class PropertyShorthandRequest {
    options map<string, string>
    retries int
    label string
}

class PropertyShorthandEnvelope {
    request PropertyShorthandRequest
}

function property_shorthand_class(
    mode: string,
    retries: int,
    label: string,
) -> PropertyShorthandRequest {
    let options = { "mode": mode };
    PropertyShorthandRequest { options, retries, label }
}

test "class properties use shorthand" {
    assert.is_true(baml.deep_equals(
        property_shorthand_class("safe", 2, "primary"),
        PropertyShorthandRequest {
            options: { "mode": "safe" },
            retries: 2,
            label: "primary",
        },
    ))
}

function property_shorthand_map(count: int, retries: int) -> map<string, int> {
    { count, retries }
}

test "map entries use shorthand" {
    let result = property_shorthand_map(7, 3);
    assert.equal(result.get("count"), 7);
    assert.equal(result.get("retries"), 3)
}

function property_shorthand_mixed(
    mode: string,
    retries: int,
    label: string,
) -> PropertyShorthandEnvelope {
    let options = { "mode": mode };
    let request = PropertyShorthandRequest {
        options,
        retries: retries + 1,
        label,
    };
    PropertyShorthandEnvelope { request }
}

test "shorthand mixes with explicit properties and nests" {
    assert.is_true(baml.deep_equals(
        property_shorthand_mixed("fast", 4, "nested"),
        PropertyShorthandEnvelope {
            request: PropertyShorthandRequest {
                options: { "mode": "fast" },
                retries: 5,
                label: "nested",
            },
        },
    ))
}
