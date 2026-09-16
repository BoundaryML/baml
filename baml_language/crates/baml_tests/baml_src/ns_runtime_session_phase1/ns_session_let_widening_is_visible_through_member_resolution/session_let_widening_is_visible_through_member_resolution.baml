function probe(s: reflect.Session, source: string) -> string {
    let _ = s.eval(source) catch (e) {
        reflect.errors.CompilationError => {
            return e.diagnostics[0].message;
        },
        _ => {
            return "wrong error";
        },
    };
    "unexpected success"
}

function main() -> string {
    let s = reflect.Session.new();
    s.eval(`let n = 5`);
    s.eval(`let text = "hi"`);
    s.eval(`let flag = true`);
    `
        ${probe(s, `n.to_string()`)}
        ${probe(s, `text.to_string()`)}
        ${probe(s, `flag.to_string()`)}
    `
}

test "session_let_widening_is_visible_through_member_resolution" {
    assert.equal(
        main(),
        "type `int` has no member `to_string`\ntype `string` has no member `to_string`\ntype `bool` has no member `to_string`",
    )
}
