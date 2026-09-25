function main() -> string {
    let s = reflect.Session.new();
    s.eval(`let n = 5`);
    s.eval<string>(`if (n is 5) { "narrowed" } else { "wide" }`)
}

test "session_let_narrowing_still_sees_the_literal" {
    assert.equal(main(), "narrowed")
}
