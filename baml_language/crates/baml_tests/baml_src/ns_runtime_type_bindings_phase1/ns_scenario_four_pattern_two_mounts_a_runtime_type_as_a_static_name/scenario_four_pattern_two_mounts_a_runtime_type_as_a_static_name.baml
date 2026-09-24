interface PersonAnchor {
    name: string,
    email: string,
}

function TenantPersonType() -> reflect.class.Type {
    let anchor_impl = reflect.interface.implementation<PersonAnchor>().field("name").field("email");
    reflect.class.new(
        "AcmePerson",
        {
            "name": reflect.Type.of<string>(),
            "email": reflect.Type.of<string>(),
            "favorite_editor": reflect.Type.of<string>(),
        },
        implementations = [anchor_impl],
    )
}

function main() -> string {
    let person_t = TenantPersonType();
    let app = reflect.Package.compile({ "empty.baml": "" }).with_types({ "AcmePerson": person_t });
    let exported = app.get_class("root.AcmePerson") ?? throw "missing mounted class";
    if (exported.as_type() != person_t.as_type()) {
        throw "with_types changed the exported type"
    }

    let pkg = reflect.Package.compile(
        {
            "tenant.baml": `
                function Run(document: string) -> app.AcmePerson {
                  app.AcmePerson {
                    name: "Ada",
                    email: "ada@example.com",
                    favorite_editor: document,
                  }
                }
            `,
        },
        packages = { "app": app },
    );

    let extract = pkg.get_function<(string) -> PersonAnchor>("root.Run") ?? throw "missing root.Run";
    let person = extract(`{"name":"Ada","email":"ada@example.com","favorite_editor":"vim"}`);
    if (reflect.Type.of_value(person) != person_t.as_type()) {
        throw "compiled wrapper did not return the mounted type"
    }
    person.name + "|" + person.email
}

test "scenario_four_pattern_two_mounts_a_runtime_type_as_a_static_name" {
    assert.equal(main(), "Ada|ada@example.com")
}
