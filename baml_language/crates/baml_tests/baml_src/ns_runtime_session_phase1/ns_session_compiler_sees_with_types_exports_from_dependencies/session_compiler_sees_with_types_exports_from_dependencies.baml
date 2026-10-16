function main() -> bool {
    let mounted = reflect.Class.new("MountedTicket", { "subject": string.type() });
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
