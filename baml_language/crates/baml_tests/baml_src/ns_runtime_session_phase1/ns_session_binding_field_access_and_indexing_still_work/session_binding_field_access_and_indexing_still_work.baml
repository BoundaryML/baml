function main() -> string {
    let s = reflect.Session.new();
    s.eval(`class Draft { title string }`);
    s.eval(`let draft = Draft { title: "titled" }`);
    s.eval(`let items = [7, 8]`);
    s.eval<string>(`` `\${draft.title}|\${items[0]}` ``)
}

test "session_binding_field_access_and_indexing_still_work" {
    assert.equal(main(), "titled|7")
}
