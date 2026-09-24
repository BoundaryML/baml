function probe(s: reflect.Session, source: string) -> string {
    let _ = s.eval(source) catch (e) {
        reflect.errors.CompilationError => {
            return `${e.diagnostics[0].code}: ${e.diagnostics[0].message}`;
        },
        baml.panics.Panic => {
            return "compiled, panicked at runtime";
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
    s.eval(`let n = "seven"`);
    s.eval<string>(`n.to_upper_case()`)
}

test "a_session_let_still_shadows_at_a_new_type" {
    assert.equal(main(), "SEVEN")
}
