function main() -> int {
    let s = reflect.Session.new();
    s.eval(`let x = 10`);
    s.eval<int>(`x + 1`)
}

test "session_commits_a_binding_and_returns_a_contracted_value" {
    assert.equal(main(), 11)
}
