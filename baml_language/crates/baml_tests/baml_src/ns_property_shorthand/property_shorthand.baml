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

class PropertyShorthandLeaf {
    value string
}

class PropertyShorthandMiddle {
    leaf PropertyShorthandLeaf
}

class PropertyShorthandOuter {
    middle PropertyShorthandMiddle
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
    assert.equal(
        property_shorthand_class("safe", 2, "primary"),
        PropertyShorthandRequest { options: { "mode": "safe" }, retries: 2, label: "primary"},
    )
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
    assert.equal(
        property_shorthand_mixed("fast", 4, "nested"),
        PropertyShorthandEnvelope { request: PropertyShorthandRequest { options: { "mode": "fast" }, retries: 5, label: "nested"}},
    )
}

function property_shorthand_multi_nested(value: string) -> PropertyShorthandOuter {
    let leaf = PropertyShorthandLeaf { value };
    let middle = PropertyShorthandMiddle { leaf };
    PropertyShorthandOuter { middle }
}

test "shorthand works across multiple nested classes" {
    assert.equal(
        property_shorthand_multi_nested("deep"),
        PropertyShorthandOuter { middle: PropertyShorthandMiddle { leaf: PropertyShorthandLeaf { value: "deep"}}},
    )
}

class PropertyShorthandBinder {
    value: string,
}

class PropertyShorthandError {
    message: string,
}

function property_shorthand_if_let(input: string?) -> string {
    if let name: string = input {
        let quoted: map<string, string> = { "name": name };
        let shorthand: map<string, string> = { name };
        (quoted.get("name") ?? "") + (shorthand.get("name") ?? "")
    } else {
        ""
    }
}

function property_shorthand_match(input: string?) -> string {
    match (input) {
        let name: string => {
            let shorthand: map<string, string> = { name };
            shorthand.get("name") ?? ""
        },
        null => "",
    }
}

function property_shorthand_for(names: string[]) -> string {
    let output = "";
    for (let name in names) {
        let shorthand: map<string, string> = { name };
        output = output + (shorthand.get("name") ?? "");
    }
    output
}

function property_shorthand_throw() -> string throws PropertyShorthandError {
    throw PropertyShorthandError { message: "caught" }
}

function property_shorthand_catch() -> string {
    property_shorthand_throw() catch (error) {
        PropertyShorthandError => {
            let shorthand = { error };
            error.message
        }
    }
}

function property_shorthand_destructure() -> string {
    let PropertyShorthandBinder { value } = PropertyShorthandBinder { value: "destructured" };
    let shorthand: map<string, string> = { value };
    shorthand.get("value") ?? ""
}

function property_shorthand_nested_block() -> string {
    {
        let name = "nested";
        let shorthand: map<string, string> = { name };
        shorthand.get("name") ?? ""
    }
}

function property_shorthand_class_if_let(input: string?) -> string {
    if let value: string = input {
        PropertyShorthandBinder { value }.value
    } else {
        ""
    }
}

test "if let binders work in explicit and shorthand map entries" {
    assert.equal(property_shorthand_if_let("if-let"), "if-letif-let")
}

test "shorthand uses every lexical binder scope" {
    assert.equal(property_shorthand_match("match"), "match");
    assert.equal(property_shorthand_for(["for", "-binder"]), "for-binder");
    assert.equal(property_shorthand_catch(), "caught");
    assert.equal(property_shorthand_destructure(), "destructured");
    assert.equal(property_shorthand_nested_block(), "nested")
}

test "class shorthand sees if let binders" {
    assert.equal(property_shorthand_class_if_let("class"), "class")
}
