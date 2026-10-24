// Which member of a union `baml.json` decodes a value as: the best match.
// Every case runs `decode<T>` from text, which reads it in one pass, and
// `to<T>` over the parsed value; nested unions also run inside a member a
// union tries, which resolves them by the same rule.

// ─── A JSON object: the class that uses the most keys ────────────────────────

class UnionEmpty {
}

class UnionCount {
    n: int @alias("count"),
}

function union_class_tag(v: UnionEmpty | UnionCount) -> string {
    match (v) {
        UnionEmpty => "Empty",
        UnionCount => "Count",
    }
}

function union_classes(
    text: string,
    naming: baml.json.Naming,
) -> string throws baml.json.ParseError | baml.json.DecodeError {
    let decoded = baml.json.decode<UnionEmpty | UnionCount>(text, naming = naming);
    let converted = baml.json.to<UnionEmpty | UnionCount>(baml.json.parse(text), naming = naming);
    union_class_tag(decoded) + "," + union_class_tag(converted)
}

test "an object decodes as the class that uses its keys (B-1841)" {
    let declared = baml.json.Naming.Declared;
    assert.equal(union_classes("{\"n\":1}", declared), "Count,Count");
    assert.equal(union_classes("{\"n\":1,\"extra\":2}", declared), "Count,Count");
    assert.equal(union_classes("{}", declared), "Empty,Empty");
    assert.equal(union_classes("{\"other\":1}", declared), "Empty,Empty")
}

test "keys count under aliased names" {
    let aliased = baml.json.Naming.Aliased;
    assert.equal(union_classes("{\"count\":1}", aliased), "Count,Count");
    assert.equal(union_classes("{\"count\":1,\"extra\":2}", aliased), "Count,Count");
    // The declared name is an unknown key under aliased names, and the alias
    // one under declared names.
    assert.equal(union_classes("{\"n\":1}", aliased), "Empty,Empty");
    assert.equal(union_classes("{\"count\":1}", baml.json.Naming.Declared), "Empty,Empty")
}

test "a class that fails deeper in falls through to the next" {
    let declared = baml.json.Naming.Declared;
    assert.equal(union_classes("{\"n\":\"one\"}", declared), "Empty,Empty");
    assert.equal(union_classes("{\"count\":\"one\"}", baml.json.Naming.Aliased), "Empty,Empty")
}

class UnionFirst {
    n: int,
}

class UnionSecond {
    n: int,
}

function union_tie(text: string) -> string throws baml.json.ParseError | baml.json.DecodeError {
    let first = baml.json.decode<UnionFirst | UnionSecond>(text);
    let first_to = baml.json.to<UnionFirst | UnionSecond>(baml.json.parse(text));
    let second = baml.json.decode<UnionSecond | UnionFirst>(text);
    let second_to = baml.json.to<UnionSecond | UnionFirst>(baml.json.parse(text));
    let tag = (v: UnionFirst | UnionSecond) -> string {
        match (v) {
            UnionFirst => "First",
            UnionSecond => "Second",
        }
    };
    tag(first) + "," + tag(first_to) + "," + tag(second) + "," + tag(second_to)
}

test "a tie goes to declaration order" {
    assert.equal(union_tie("{\"n\":1}"), "First,First,Second,Second")
}

// ─── B-1151: an event read back is the event written ─────────────────────────

class PolicyChanged {
    policy: string?,
    metadata: map<string, json>?,
}

class UserMessage {
    content: string,
    metadata: map<string, json>,
}

type ConversationEvent = PolicyChanged | UserMessage;

function event_tag(event: ConversationEvent) -> string {
    match (event) {
        PolicyChanged => "PolicyChanged",
        UserMessage => "UserMessage",
    }
}

test "a user message round trips as a user message (B-1151)" {
    let event: ConversationEvent = UserMessage { content: "hi", metadata: { "k": 1 } };
    let text = baml.json.stringify(event);
    assert.equal(event_tag(baml.json.decode<ConversationEvent>(text)), "UserMessage");
    assert.equal(event_tag(baml.json.to<ConversationEvent>(baml.json.from(event))), "UserMessage");
    let changed: ConversationEvent = PolicyChanged { policy: "strict", metadata: null };
    assert.equal(
        event_tag(baml.json.decode<ConversationEvent>(baml.json.stringify(changed))),
        "PolicyChanged",
    )
}

// ─── Maps, and the members that take any value ───────────────────────────────

function union_map_tag(v: UnionCount | map<string, int>) -> string {
    match (v) {
        UnionCount => "Count",
        _ => "map",
    }
}

function union_maps(
    text: string,
    naming: baml.json.Naming,
) -> string throws baml.json.ParseError | baml.json.DecodeError {
    let decoded = baml.json.decode<UnionCount | map<string, int>>(text, naming = naming);
    let converted = baml.json.to<UnionCount | map<string, int>>(baml.json.parse(text), naming = naming);
    union_map_tag(decoded) + "," + union_map_tag(converted)
}

test "a map uses every key, and a tie with a class goes to declaration order" {
    let declared = baml.json.Naming.Declared;
    assert.equal(union_maps("{\"n\":1,\"m\":2}", declared), "map,map");
    assert.equal(union_maps("{\"n\":1}", declared), "Count,Count");
    let aliased = baml.json.Naming.Aliased;
    assert.equal(union_maps("{\"count\":1,\"m\":2}", aliased), "map,map");
    assert.equal(union_maps("{\"count\":1}", aliased), "Count,Count");
    // Under aliased names `n` is not the class's key, so the map wins.
    assert.equal(union_maps("{\"n\":1}", aliased), "map,map")
}

function union_any_tag(v: UnionCount | json) -> string {
    match (v) {
        UnionCount => "Count",
        _ => "json",
    }
}

function union_open_map_tag(v: map<string, json> | UnionCount) -> string {
    match (v) {
        UnionCount => "Count",
        _ => "map",
    }
}

function union_unknown_tag(v: unknown) -> string {
    match (v) {
        UnionCount => "Count",
        _ => "unknown",
    }
}

function union_any(
    text: string,
    naming: baml.json.Naming,
) -> string throws baml.json.ParseError | baml.json.DecodeError {
    let parsed = baml.json.parse(text);
    let decoded = baml.json.decode<UnionCount | json>(text, naming = naming);
    let converted = baml.json.to<UnionCount | json>(parsed, naming = naming);
    let open = baml.json.decode<map<string, json> | UnionCount>(text, naming = naming);
    let open_to = baml.json.to<map<string, json> | UnionCount>(parsed, naming = naming);
    let unknown_first = baml.json.decode<unknown | UnionCount>(text, naming = naming);
    let unknown_to = baml.json.to<unknown | UnionCount>(parsed, naming = naming);
    union_any_tag(decoded)
        + ","
        + union_any_tag(converted)
        + ","
        + union_open_map_tag(open)
        + ","
        + union_open_map_tag(open_to)
        + ","
        + union_unknown_tag(unknown_first)
        + ","
        + union_unknown_tag(unknown_to)
}

test "json, unknown, and a map of json come last" {
    // An extra key does not make `json` or `map<string, json>` the better
    // match: they take any object.
    let declared = baml.json.Naming.Declared;
    assert.equal(
        union_any("{\"n\":1,\"extra\":2}", declared),
        "Count,Count,Count,Count,Count,Count",
    );
    assert.equal(union_any("{\"other\":1}", declared), "json,json,map,map,unknown,unknown");
    let aliased = baml.json.Naming.Aliased;
    assert.equal(
        union_any("{\"count\":1,\"extra\":2}", aliased),
        "Count,Count,Count,Count,Count,Count",
    );
    assert.equal(union_any("{\"n\":1}", aliased), "json,json,map,map,unknown,unknown")
}

function bigint_or_string(
    text: string,
    naming: baml.json.Naming,
) -> string throws baml.json.ParseError | baml.json.DecodeError {
    let tag = (v: bigint | string) -> string {
        match (v) {
            bigint => "bigint",
            _ => "string",
        }
    };
    let parsed = baml.json.parse(text);
    tag(baml.json.decode<bigint | string>(text, naming = naming))
        + ","
        + tag(baml.json.to<bigint | string>(parsed, naming = naming))
        + ","
        + tag(baml.json.decode<string | bigint>(text, naming = naming))
        + ","
        + tag(baml.json.to<string | bigint>(parsed, naming = naming))
}

test "a numeric string ties between bigint and string" {
    assert.equal(
        bigint_or_string("\"42\"", baml.json.Naming.Declared),
        "bigint,bigint,string,string",
    );
    assert.equal(
        bigint_or_string("\"42\"", baml.json.Naming.Aliased),
        "bigint,bigint,string,string",
    );
    // A number is only ever the bigint.
    assert.equal(bigint_or_string("42", baml.json.Naming.Declared), "bigint,bigint,bigint,bigint")
}

test "unknown loses to a more specific member" {
    let n = baml.json.decode<unknown | int>("7");
    assert.equal(n is int, true);
    let s = baml.json.decode<json | string>("\"x\"");
    assert.equal(s is string, true)
}

// ─── Scalars: the most specific type ─────────────────────────────────────────

function number_tag(v: float | int | bigint) -> string {
    match (v) {
        let i: int => "int " + string.from(i),
        let b: bigint => "bigint " + string.from(b),
        let f: float => "float " + string.from(f),
    }
}

function numbers(text: string) -> string throws baml.json.ParseError | baml.json.DecodeError {
    let decoded = baml.json.decode<float | bigint | int>(text);
    let converted = baml.json.to<float | bigint | int>(baml.json.parse(text));
    number_tag(decoded) + "," + number_tag(converted)
}

test "an integral number is an int when it fits (B-1841)" {
    assert.equal(numbers("1"), "int 1,int 1");
    assert.equal(
        numbers("99999999999999999999"),
        "bigint 99999999999999999999,bigint 99999999999999999999",
    );
    assert.equal(numbers("1.5"), "float 1.5,float 1.5");
    assert.equal(numbers("1.0"), "float 1.0,float 1.0");
    let one = baml.json.decode<float | int>("1");
    assert.equal(one is int, true)
}

function literal_tag(v: 1 | int) -> string {
    match (v) {
        1 => "literal",
        _ => "int",
    }
}

test "a literal comes before its base type" {
    assert.equal(literal_tag(baml.json.decode<int | 1>("1")), "literal");
    assert.equal(literal_tag(baml.json.decode<int | 1>("2")), "int")
}

enum UnionColor {
    Red @alias("rouge"),
    Green,
}

function color_tag(v: string | UnionColor) -> string {
    match (v) {
        let c: UnionColor => "Color." + string.from(c),
        let s: string => "string " + s,
    }
}

function colors(
    text: string,
    naming: baml.json.Naming,
) -> string throws baml.json.ParseError | baml.json.DecodeError {
    let decoded = baml.json.decode<string | UnionColor>(text, naming = naming);
    let converted = baml.json.to<string | UnionColor>(baml.json.parse(text), naming = naming);
    color_tag(decoded) + "," + color_tag(converted)
}

test "a string that names a variant is the enum" {
    let declared = baml.json.Naming.Declared;
    assert.equal(colors("\"Red\"", declared), "Color.Red,Color.Red");
    assert.equal(colors("\"purple\"", declared), "string purple,string purple");
    assert.equal(colors("\"rouge\"", declared), "string rouge,string rouge")
}

test "a variant's alias names it under aliased names" {
    let aliased = baml.json.Naming.Aliased;
    assert.equal(colors("\"rouge\"", aliased), "Color.Red,Color.Red");
    assert.equal(colors("\"Green\"", aliased), "Color.Green,Color.Green");
    assert.equal(colors("\"Red\"", aliased), "string Red,string Red")
}

function word_tag(v: "red" | UnionColor | string) -> string {
    match (v) {
        "red" => "literal",
        let c: UnionColor => "Color." + string.from(c),
        _ => "string",
    }
}

test "a string literal comes before an enum and a string" {
    assert.equal(word_tag(baml.json.decode<string | UnionColor | "red">("\"red\"")), "literal");
    assert.equal(word_tag(baml.json.decode<string | UnionColor | "red">("\"Red\"")), "Color.Red")
}

// ─── Nested unions resolve by the same rule ──────────────────────────────────

class UnionNested {
    shape: UnionEmpty | UnionCount,
    number: float | int,
    color: string | UnionColor,
}

function nested_tag(v: UnionNested) -> string {
    union_class_tag(v.shape) + " " + number_tag(v.number) + " " + color_tag(v.color)
}

function nested_or_int(v: UnionNested | int) -> string {
    match (v) {
        let nested: UnionNested => nested_tag(nested),
        _ => "int",
    }
}

test "a union inside a class resolves by the same rule" {
    let text = "{\"shape\":{\"n\":1},\"number\":2,\"color\":\"Green\"}";
    let expected = "Count int 2 Color.Green";
    assert.equal(nested_tag(baml.json.decode<UnionNested>(text)), expected);
    assert.equal(nested_tag(baml.json.to<UnionNested>(baml.json.parse(text))), expected);
    // Inside a member the outer union tries.
    assert.equal(nested_or_int(baml.json.decode<UnionNested | int>(text)), expected);
    assert.equal(nested_or_int(baml.json.to<UnionNested | int>(baml.json.parse(text))), expected)
}

test "a union inside a class resolves under aliased names" {
    let text = "{\"shape\":{\"count\":1},\"number\":2,\"color\":\"rouge\"}";
    let aliased = baml.json.Naming.Aliased;
    let expected = "Count int 2 Color.Red";
    assert.equal(nested_tag(baml.json.decode<UnionNested>(text, naming = aliased)), expected);
    assert.equal(
        nested_or_int(baml.json.to<UnionNested | int>(baml.json.parse(text), naming = aliased)),
        expected,
    )
}

// ─── When no member fits ─────────────────────────────────────────────────────

class UnionBlob {
    bytes: uint8array,
}

class UnionMessage {
    content: string | uint8array,
}

function decode_error<T>(text: string) -> string {
    {
        let _ = baml.json.decode<T>(text);
        "decoded"
    } catch (e) {
        baml.json.DecodeError => e.message + " at '" + e.path + "'",
        baml.json.ParseError => "parse error",
    }
}

function to_error<T>(text: string) -> string {
    {
        let parsed = baml.json.parse(text);
        let _ = baml.json.to<T>(parsed);
        "decoded"
    } catch (e) {
        baml.json.DecodeError => e.message + " at '" + e.path + "'",
        baml.json.ParseError => "parse error",
    }
}

test "the error names a member dropped because it has no JSON form" {
    let expected = "expected string (`uint8array` is not possible here: it has no JSON form) at '.content'";
    assert.equal(decode_error<UnionMessage>("{\"content\":5}"), expected);
    assert.equal(to_error<UnionMessage>("{\"content\":5}"), expected)
}

test "the error names each member's misfit" {
    let expected = "no union member matched: `json_union_match.UnionCount`: missing required field `n`; `int`: expected integer at ''";
    assert.equal(decode_error<UnionCount | int>("{\"m\":1}"), expected);
    assert.equal(to_error<UnionCount | int>("{\"m\":1}"), expected)
}

test "the error names a dropped class and why" {
    let message = to_error<UnionBlob | int>("\"x\"");
    assert.equal(
        message,
        "expected integer (`json_union_match.UnionBlob` is not possible here: field `json_union_match.UnionBlob.bytes`: `uint8array` has no JSON form) at ''",
    )
}
