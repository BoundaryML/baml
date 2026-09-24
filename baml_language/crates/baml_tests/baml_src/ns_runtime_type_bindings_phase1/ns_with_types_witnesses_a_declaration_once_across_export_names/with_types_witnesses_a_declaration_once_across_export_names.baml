/// A runtime class mounted under TWO export names is one declaration: the
/// export name is another spelling of it, not a second identity, so its
/// witness (the `Labelled` implementation) is served once and both spellings
/// satisfy the interface in a consumer. Serving the witness per export name
/// would export one impl identity twice, which the mount boundary refuses.
function main() -> bool {
    let base = reflect.Package.compile({ "base.baml": `interface Labelled { label string }` });
    let labelled = base.get_interface("root.Labelled") ?? throw "missing Labelled";
    type MountedLabelled = unreflect(labelled.as_type())
    let witness = reflect.interface.implementation<MountedLabelled>().field("label", class_field = "display_name");
    let person = reflect.class.new(
        "InternalPerson",
        { "display_name": reflect.Type.of<string>() },
        implementations = [witness],
    );
    let app = base.with_types({ "PersonAlias": person, "PersonTwin": person });
    let compiled = reflect.Package.compile(
        {
            "consumer.baml": `
                function as_label(value: app.PersonAlias) -> app.Labelled {
                  value
                }
                function twin_as_label(value: app.PersonTwin) -> app.Labelled {
                  value
                }
            `,
        },
        packages = { "app": app },
    );
    compiled.functions().get("root.as_label") != null
        && compiled.functions().get("root.twin_as_label") != null
}

test "with_types_witnesses_a_declaration_once_across_export_names" {
    assert.equal(main(), true)
}
