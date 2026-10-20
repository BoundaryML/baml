interface Labelled {
    label: string,
}

function main() -> bool {
    let base = reflect.Package.compile({ "base.baml": `interface Labelled { label string }` });
    let labelled = base.get_interface("root.Labelled");
    let type MountedLabelled = labelled;
    let witness = reflect.Interface.implementation<MountedLabelled>().field("label", class_field = "display_name");
    let person = reflect.Class.new(
        "InternalPerson",
        { "display_name": string.type() },
        implementations = [witness],
    );
    let app = base.with_types({ "PersonAlias": person });
    let compiled = reflect.Package.compile(
        {
            "consumer.baml": `
                function as_label(value: app.PersonAlias) -> app.Labelled {
                  value
                }
            `,
        },
        packages = { "app": app },
    );
    compiled.functions().get("root.as_label") != null
}

test "with_types_carries_runtime_witnesses_into_consumer_compilation" {
    assert.equal(main(), true)
}
