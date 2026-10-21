test "b1470_bound_method_on_session_global" {
    let session = reflect.Session.new()
    session.eval(`
        class Greeter {
            name string
            function greet(self) -> string throws never {
                "hi " + self.name
            }
        }
    `)
    session.eval(`let greeter = Greeter { name: "ada" }`)
    session.eval(`let greet = greeter.greet`)
    assert.equal(session.eval<string>(`greet()`), "hi ada")
}

test "b1470_external_bound_method_on_session_global" {
    let app = reflect.Package.compile({
        "greeter.baml": `
            class Greeter {
                name string
                function greet(self) -> string throws never {
                    "hi " + self.name
                }
            }
        `
    })
    let session = reflect.Session.new(packages = { "app": app })
    session.eval(`let greeter = app.Greeter { name: "ada" }`)
    session.eval(`let greet = greeter.greet`)
    assert.equal(session.eval<string>(`greet()`), "hi ada")
}
