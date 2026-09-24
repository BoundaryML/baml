function main() -> string {
    let node = reflect.class.builder("Node");
    let next = node.type().optional();
    node.field("label", reflect.Type.of<string>());
    node.field("next", next.meta(alias = "child", description = "Recursive child"));

    let built = node.build().as_type().as_class() ?? throw "expected class";
    let fields = built.fields();
    let label = fields.at(0) ?? throw "expected label field";
    let child = fields.at(1) ?? throw "expected next field";

    let label_alias = label.meta.alias ?? "none";
    let child_alias = child.meta.alias ?? "none";
    let child_description = child.meta.description ?? "none";
    `${label.name}|${label_alias}|${child.name}|${child_alias}|${child_description}`
}

test "recursive_pending_fields_carry_metadata" {
    assert.equal(main(), "label|none|next|child|Recursive child")
}
