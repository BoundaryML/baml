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
    s.eval(`let m = { "k": 1 }`);
    s.eval(`let text = "hi"`);
    s.eval(`m = { "k": 2 }`);
    s.eval(``text = `\${text} there```);
    s.eval<string>(`` `\${m["k"]}|\${text}` ``)
}

test "session_assignment_values_keep_their_shape" {
    assert.equal(main(), "2|hi there")
}
