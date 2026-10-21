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
    s.eval(
        `
            let target_1 = 99
            n = target_1
        `,
    );
    let after = s.eval<int>(`n`);
    `${after}`
}

test "a_session_assignment_does_not_shadow_a_user_binding" {
    assert.equal(main(), "99")
}
