function main() -> bool {
    let mounted = reflect.class.new("MountedTicket", { "subject": reflect.Type.of<string>() });
    let app = reflect.Package.compile({ "empty.baml": "" }).with_types({ "MountedTicket": mounted });
    let s = reflect.Session.new(packages = { "app": app });
    s.eval(
        `
            let ticket = app.MountedTicket { subject: "visible" }
            ticket.subject
        `,
    )
        == "visible"
}

test "session_compiler_sees_with_types_exports_from_dependencies" {
    assert.equal(main(), true)
}
