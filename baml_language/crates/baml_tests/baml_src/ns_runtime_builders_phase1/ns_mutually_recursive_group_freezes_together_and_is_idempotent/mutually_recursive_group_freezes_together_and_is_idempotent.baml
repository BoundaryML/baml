function main() -> string {
    let left = reflect.class.builder("Left");
    let right = reflect.class.builder("Right");
    let left_p = left.type();
    let right_p = right.type();
    let lefts_p = left_p.array();
    let either_p = left_p.union([reflect.Type.of<string>()]);
    let pending_starts_null = left_p.resolved() == null;

    left.field("right", right_p.optional());
    right.field("lefts", lefts_p);
    right.field("either", either_p);

    let left_t = left.build();
    let right_t = right.build();
    let left_again = left.build();
    let right_again = right.build();
    let right_fields = right_t.fields();
    return pending_starts_null.to_string()
        + "|"
        + (left_t.as_type() == left_again.as_type()).to_string()
        + "|"
        + (right_t.as_type() == right_again.as_type()).to_string()
        + "|"
        + (left_p.resolved() == left_t.as_type()).to_string()
        + "|"
        + (right_p.resolved() == right_t.as_type()).to_string()
        + "|"
        + (lefts_p.resolved() == right_fields[0].type).to_string()
        + "|"
        + (either_p.resolved() == right_fields[1].type).to_string();
}

test "mutually_recursive_group_freezes_together_and_is_idempotent" {
    assert.equal(main(), "true|true|true|true|true|true|true")
}
