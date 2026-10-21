function main() -> string {
    let s = reflect.Session.new();
    s.eval(`let n = 5`);
    s.eval(`let text = "hi"`);
    s.eval(`let flag = true`);
    s.eval(`n = 7`);
    s.eval(`text = "there"`);
    s.eval(`flag = false`);
    s.eval<string>(`` `\${n}|\${text}|\${flag}` ``)
}

test "session_let_rebinding_across_submissions_is_unaffected" {
    assert.equal(main(), "7|there|false")
}
