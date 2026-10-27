// A primitive renders the same text on every road: a template hole and a
// `string.from` of a primitive type skip the generic renderer, a generic or
// `unknown` value takes the native fast path, and a nested one goes through the
// structural walker. These tests pin that text.

function generic_from<T>(value: T) -> string throws never {
    string.from(value)
}

function unknown_from(value: unknown) -> string throws never {
    string.from(value)
}

function int_roads(k: int) -> string[] {
    [`${k}`, string.from(k), k.to_string(), generic_from(k), unknown_from(k), string.from([k])]
}

function float_roads(f: float) -> string[] {
    [`${f}`, string.from(f), f.to_string(), generic_from(f), unknown_from(f), string.from([f])]
}

function bool_roads(b: bool) -> string[] {
    [`${b}`, string.from(b), b.to_string(), generic_from(b), unknown_from(b), string.from([b])]
}

function string_roads(s: string) -> string[] {
    [`${s}`, string.from(s), s.to_string(), generic_from(s), unknown_from(s), `<${s}>`]
}

test "int_text" {
    assert.equal(int_roads(0), ["0", "0", "0", "0", "0", "[0]"]);
    assert.equal(int_roads(-7), ["-7", "-7", "-7", "-7", "-7", "[-7]"]);
    assert.equal(
        int_roads(int.max_value()),
        [
            "4611686018427387903",
            "4611686018427387903",
            "4611686018427387903",
            "4611686018427387903",
            "4611686018427387903",
            "[4611686018427387903]",
        ],
    );
    assert.equal(
        int_roads(int.min_value()),
        [
            "-4611686018427387904",
            "-4611686018427387904",
            "-4611686018427387904",
            "-4611686018427387904",
            "-4611686018427387904",
            "[-4611686018427387904]",
        ],
    )
}

test "float_text" {
    assert.equal(float_roads(3.5), ["3.5", "3.5", "3.5", "3.5", "3.5", "[3.5]"]);
    assert.equal(float_roads(2.0), ["2.0", "2.0", "2.0", "2.0", "2.0", "[2.0]"]);
    assert.equal(float_roads(-0.0), ["-0.0", "-0.0", "-0.0", "-0.0", "-0.0", "[-0.0]"]);
    assert.equal(
        float_roads(0.1 + 0.2),
        [
            "0.30000000000000004",
            "0.30000000000000004",
            "0.30000000000000004",
            "0.30000000000000004",
            "0.30000000000000004",
            "[0.30000000000000004]",
        ],
    );
    assert.equal(
        float_roads(1.5e-7),
        ["0.00000015", "0.00000015", "0.00000015", "0.00000015", "0.00000015", "[0.00000015]"],
    );
    assert.equal(
        float_roads(1e21),
        [
            "1000000000000000000000.0",
            "1000000000000000000000.0",
            "1000000000000000000000.0",
            "1000000000000000000000.0",
            "1000000000000000000000.0",
            "[1000000000000000000000.0]",
        ],
    );
    assert.equal(float_roads(float.nan()), ["NaN", "NaN", "NaN", "NaN", "NaN", "[NaN]"]);
    assert.equal(
        float_roads(float.inf()),
        ["Infinity", "Infinity", "Infinity", "Infinity", "Infinity", "[Infinity]"],
    );
    assert.equal(
        float_roads(-float.inf()),
        ["-Infinity", "-Infinity", "-Infinity", "-Infinity", "-Infinity", "[-Infinity]"],
    )
}

test "bool_and_null_text" {
    assert.equal(bool_roads(true), ["true", "true", "true", "true", "true", "[true]"]);
    assert.equal(bool_roads(false), ["false", "false", "false", "false", "false", "[false]"]);
    assert.equal(
        [string.from(null), generic_from(null), unknown_from(null)],
        ["null", "null", "null"],
    )
}

test "string_text" {
    assert.equal(
        string_roads("héllo 😀 世界"),
        [
            "héllo 😀 世界",
            "héllo 😀 世界",
            "héllo 😀 世界",
            "héllo 😀 世界",
            "héllo 😀 世界",
            "<héllo 😀 世界>",
        ],
    );
    assert.equal(string_roads(""), ["", "", "", "", "", "<>"]);
    assert.equal(
        string_roads("say \"hi\"\n"),
        [
            "say \"hi\"\n",
            "say \"hi\"\n",
            "say \"hi\"\n",
            "say \"hi\"\n",
            "say \"hi\"\n",
            "<say \"hi\"\n>",
        ],
    );
    // Nested, a string is quoted.
    assert.equal(
        string.from(["héllo 😀", "say \"hi\"\n"]),
        "[\"héllo 😀\", \"say \\\"hi\\\"\\n\"]",
    )
}

test "literal_holes" {
    let tag: "x" | "y" = "y";
    assert.equal(`${tag}-${42}-${true}-${1.5}-${"lit"}`, "y-42-true-1.5-lit")
}
