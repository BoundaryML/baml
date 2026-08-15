//! BEP-066 C-3/C-8/C-9b regressions: builder witness parity and resolved
//! pending reuse across builder groups.

use baml_tests::baml_test;
use bex_engine::BexExternalValue;

#[tokio::test]
async fn builder_build_with_implementations_validates_then_freezes_atomically() {
    let output = baml_test!(
        r#"
        interface Named {
            name string
        }

        function main() -> string {
            let person = reflect.class.builder("BuiltPerson")
            let address = reflect.class.builder("BuiltAddress")
            let person_pending = person.type()
            let address_pending = address.type()
            person.field("name", type.of<string>())
            person.field("address", address_pending)
            address.field("owner", person_pending.optional())

            let incomplete = reflect.interface.implementation<Named>()
            let failed = person.build(implementations = [incomplete]) catch (e) {
                baml.reflect.errors.CompilationError => e.diagnostics[0].code
            }
            // Failed witness validation must not freeze any member of the group.
            address.field("zip", type.of<int>())

            let complete = incomplete.field("name")
            let person_t = person.build(implementations = [complete])
            let address_t = address.build()
            let person_again = person.build(implementations = [complete])
            let frozen_error = address.field("late", type.of<bool>()) catch (e) {
                baml.reflect.errors.CompilationError => e.diagnostics[0].message
            }

            let failure_code = "missing witness did not fail"
            if failed is string {
                failure_code = failed
            }
            let frozen_message = "connected builder did not freeze"
            if frozen_error is string {
                frozen_message = frozen_error
            }
            return failure_code
                + "|" + person_t.as_type().implements(type.of<Named>()).to_string()
                + "|" + (person_pending.resolved() == person_t.as_type()).to_string()
                + "|" + (address_pending.resolved() == address_t.as_type()).to_string()
                + "|" + (person_again.as_type() == person_t.as_type()).to_string()
                + "|" + frozen_message
        }
        "#
    );

    let BexExternalValue::String(result) = output
        .result
        .expect("builder witness validation and retry should be catchable")
    else {
        panic!("expected a string result")
    };
    assert!(
        result.starts_with("E0001|true|true|true|true|"),
        "builder witness validation or atomic freeze changed: {result}"
    );
    assert!(
        result.ends_with("`BuiltAddress` is frozen and cannot be mutated"),
        "connected builder was not frozen: {result}"
    );
}

#[tokio::test]
async fn map_and_builder_witnessed_classes_are_definition_equal() {
    let output = baml_test!(
        r#"
        interface Named {
            name string
        }

        function main() -> bool {
            let witness = reflect.interface.implementation<Named>()
                .field("name", class_field = "display_name")
            let map_t = reflect.class.new("Person", {
                "display_name": type.of<string>().meta(description = "public name"),
            }, implementations = [witness])

            let builder = reflect.class.builder("Person")
            builder.field(
                "display_name",
                type.of<string>().meta(description = "public name"),
            )
            let builder_t = builder.build(implementations = [witness])

            return map_t.as_type() != builder_t.as_type()
                && map_t.as_type().to_baml() == builder_t.as_type().to_baml()
                && map_t.as_type().implements(type.of<Named>())
                && builder_t.as_type().implements(type.of<Named>())
        }
        "#
    );

    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
async fn resolved_pending_from_frozen_group_is_accepted_by_another_builder() {
    let output = baml_test!(
        r#"
        function main() -> bool {
            let child = reflect.class.builder("ResolvedChild")
            let child_pending = child.type()
            child.field("value", type.of<string>())
            let child_t = child.build()

            let parent = reflect.class.builder("Parent")
            parent.field("child", child_pending)
            let parent_t = parent.build()

            return child_pending.resolved() == child_t.as_type()
                && parent_t.fields()[0].type == child_t.as_type()
        }
        "#
    );

    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}
