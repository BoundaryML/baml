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
        ${probe(s, `n.missing_method()`)}
        ${probe(s, `text.missing_method()`)}
        ${probe(s, `flag.missing_method()`)}
    `
}

test "session_let_widening_is_visible_through_member_resolution" {
    assert.equal(
        main(),
        "type `int` has no member `missing_method`\ntype `string` has no member `missing_method`\ntype `bool` has no member `missing_method`",
    )
}
