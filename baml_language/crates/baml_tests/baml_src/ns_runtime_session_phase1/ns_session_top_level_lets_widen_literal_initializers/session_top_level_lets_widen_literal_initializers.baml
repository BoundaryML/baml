function main() -> string {
    let s = reflect.Session.new();
    s.eval(`let n = 5`);
    let _ = s.eval<5>(`n`) catch (e) {
        reflect.errors.CompilationError => {
            return e.diagnostics[0].message;
        },
        _ => {
            return "wrong error";
        },
    };
    "accepted"
}

test "session_top_level_lets_widen_literal_initializers" {
    assert.equal(
        main(),
        "submission result has type `int`, which is not a subtype of requested contract `5`",
    )
}
