// BUG 5: same defect class for the `type` leaf — `v is type` constant false,
// and a type|int value matched no arm at all.
function bug5() -> string {
    let v: type | int = reflect.type_of<int>();
    if (v is type) {
        "type"
    } else if (v is int) {
        "int"
    } else {
        "neither"    // pre-fix result
    }
}

test "bug5_type_narrowing" {
    assert.equal(bug5(), "type")
}
