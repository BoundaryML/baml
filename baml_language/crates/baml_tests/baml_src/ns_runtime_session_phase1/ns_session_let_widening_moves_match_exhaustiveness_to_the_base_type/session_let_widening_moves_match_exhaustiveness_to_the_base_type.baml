function probe(s: reflect.Session, source: string) -> string {
    let _ = s.eval(source) catch (e) {
        reflect.errors.CompilationError => {
            return e.diagnostics[0].message;
        },
        _ => {
            return "wrong error";
        },
    };
    "accepted"
}

function main() -> string {
    let s = reflect.Session.new();
    s.eval(`let n = 5`);
    s.eval(`let flag = true`);
    `
        ${probe(s, `match (n) { 5 => "five" }`)}
        ${probe(s, `match (flag) { true => "t", false => "f" }`)}
    `
}

test "session_let_widening_moves_match_exhaustiveness_to_the_base_type" {
    assert.equal(main(), "non-exhaustive match on type int; missing: _\naccepted")
}
