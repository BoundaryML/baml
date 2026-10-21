function main() -> bool {
    let s = reflect.Session.new();
    let diagnosed = false;
    let _ = s.eval(
        `
            let a = b
            let b = a
        `,
    ) catch (e) {
        reflect.errors.CompilationError => {
            diagnosed = e.diagnostics.length() > 0
        },
        _ => null,
    };
    diagnosed && s.eval<int>(`1`) == 1
}

test "mutually_recursive_session_lets_diagnose_without_panicking" {
    assert.equal(main(), true)
}
