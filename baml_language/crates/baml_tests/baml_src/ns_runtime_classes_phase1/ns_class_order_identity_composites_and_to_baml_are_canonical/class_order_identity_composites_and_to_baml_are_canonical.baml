function main() -> string {
    let left = reflect.Class.new(
        "Ordered",
        { "a": int.type(), "b": string.type().meta(description = "second") },
    );
    let right = reflect.Class.new(
        "Ordered",
        { "b": string.type().meta(description = "second"), "a": int.type() },
    );
    let literal = reflect.Literal.new("fixed");
    let union = reflect.Union.new([literal.as_type(), int.type()]);
    let map_t = reflect.Map.new(string.type(), union.as_type());
    let array_t = map_t.as_type().array();
    let optional_t = array_t.as_type().optional();
    let empty = reflect.Class.new("Empty", {});
    let source = left.as_type().to_baml();
    return source
        + "\n<RIGHT>\n"
        + right.as_type().to_baml()
        + "\n<OPTIONAL>\n"
        + optional_t.as_type().to_baml()
        + "\n<FLAGS>"
        + (left != right).to_string()
        + "|"
        + (source == left.as_type().to_baml()).to_string()
        + "|"
        + (literal.as_type().as_literal() != null).to_string()
        + "|"
        + (union.as_type().as_union() != null).to_string()
        + "|"
        + (map_t.as_type().as_map() != null).to_string()
        + "|"
        + (array_t.as_type().as_array() != null).to_string()
        + "|"
        + (optional_t.as_type().as_union() != null).to_string()
        + "|"
        + (empty.fields().length() == 0).to_string();
}

test "class_order_identity_composites_and_to_baml_are_canonical" {
    assert.equal(
        main(),
        "class Ordered {\n  a int\n  b string @description(\"second\")\n}\n<RIGHT>\nclass Ordered {\n  b string @description(\"second\")\n  a int\n}\n<OPTIONAL>\ntype RuntimeType = map<string, \"fixed\" | int>[]?\n<FLAGS>true|true|true|true|true|true|true|true",
    )
}
