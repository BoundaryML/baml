function main() -> bool {
    let s = reflect.Session.new();
    s.eval(`let json = "local"`);
    let local = s.eval<string>(`json`);
    let encoded = s.eval<string>(`json.stringify(7)`);
    local == "local" && encoded == "7"
}

test "session_let_named_json_does_not_shadow_json_package_paths" {
    assert.equal(main(), true)
}
