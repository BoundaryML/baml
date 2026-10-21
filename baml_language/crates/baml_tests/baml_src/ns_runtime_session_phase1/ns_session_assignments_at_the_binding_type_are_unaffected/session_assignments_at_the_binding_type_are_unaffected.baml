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
    s.eval(`let text = "hi"`);
    s.eval(`n = n + 2`);
    s.eval(`text += " there"`);
    s.eval<string>(`` `\${n}|\${text}` ``)
}

test "session_assignments_at_the_binding_type_are_unaffected" {
    assert.equal(main(), "7|hi there")
}
