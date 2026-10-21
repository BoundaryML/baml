function inspect() -> unknown {
    let app = reflect.Package.compile(
        {
            "app.baml": `
                function LoadNotes() -> string { "raw notes" }
            `,
        },
    );
    let s = reflect.Session.new(packages = { "app": app });
    s.eval(`class Draft { title string, body string }`);
    s.eval(`let draft = Draft { title: "title", body: app.LoadNotes() }`);
    s.eval(`draft.body`)
}

test "session_reimports_instance_fields_without_changing_the_value" {
    assert.equal(inspect(), "raw notes")
}
