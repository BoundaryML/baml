// BEP-066 I-11: canonical source retains witness-backed conformance across
// to_baml() and reflect.Package.compile().

test "witnessed_to_baml_round_trip_preserves_the_definition_tuple" {
    let witness = reflect.interface.implementation<root.Named<Value = string>>().field(
        "name",
        class_field = "display_name",
    );
    let original = reflect.class.new(
        "Person",
        { "display_name": reflect.Type.of<string>() },
        implementations = [witness],
    );
    let source = original.as_type().to_baml();
    let checked_source = source
        + `

            function as_named(value: Person) -> Named<Value = string> {
              value
            }
        `;
    let compiled_package = reflect.Package.compile(
        {
            "contract.baml": `
                interface Named {
                    type Value
                    name Self.Value
                }
            `,
            "person.baml": checked_source,
        },
    );
    let compiled = compiled_package.get_class("root.Person") ?? throw "missing compiled Person";

    assert.is_true(original.as_type().implements(reflect.Type.of<root.Named<Value = string>>()));
    assert.is_true(original.as_type() != compiled.as_type());
    assert.equal(compiled.fields()[0].name, "display_name");
    assert.not_null(compiled_package.functions().get("root.as_named"))
}

test "same_named_runtime_declarations_are_rejected" {
    let first = reflect.class.new("Choice", { "left": reflect.Type.of<string>() });
    let second = reflect.class.new("Choice", { "right": reflect.Type.of<int>() });
    let combined = reflect.union.new([first.as_type(), second.as_type()]);
    let result = combined.as_type().to_baml() catch (e) {
        reflect.errors.CompilationError => e.diagnostics[0].code,
        _ => "wrong error",
    };
    assert.equal(result, "E0162")
}

test "minted_and_compiled_same_named_declarations_are_rejected" {
    let package = reflect.Package.compile({ "original.baml": `class Choice { compiled string }` });
    let compiled_choice = package.get_class("root.Choice") ?? throw "missing Choice";
    let minted_choice = reflect.class.new("Choice", { "minted": reflect.Type.of<int>() });
    let combined = reflect.union.new([compiled_choice.as_type(), minted_choice.as_type()]);
    let result = combined.as_type().to_baml() catch (e) {
        reflect.errors.CompilationError => e.diagnostics[0].code,
        _ => "wrong error",
    };
    assert.equal(result, "E0162")
}

test "equivalent_same_named_runtime_declarations_fold_and_round_trip" {
    let first = reflect.class.new("Choice", { "value": reflect.Type.of<string>() });
    let second = reflect.class.new("Choice", { "value": reflect.Type.of<string>() });
    let combined = reflect.union.new([first.as_type(), second.as_type()]);
    let source = combined.as_type().to_baml();
    let round_trip = reflect.Package.compile({ "choices.baml": source });
    assert.equal(source.split("class Choice {").length(), 2);
    assert.not_null(round_trip.get_class("root.Choice"));
    assert.is_true(round_trip.get_class("root.Choice_2") == null);
    assert.equal(round_trip.classes().length(), 1)
}

test "runtime_class_static_dependencies_are_declared_in_round_trip_source" {
    let holder = reflect.class.new("RuntimeHolder", { "child": reflect.Type.of<root.StaticChild>() });
    let source = holder.as_type().to_baml();
    let round_trip = reflect.Package.compile({ "holder.baml": source });
    assert.not_null(round_trip.get_class("root.RuntimeHolder"));
    assert.not_null(round_trip.get_class("root.StaticChild"))
}
