// BEP-066 C-3/C-8/C-9b regressions: builder witness parity and resolved
// pending reuse across builder groups. These are native equivalents of the
// former Rust `builder_witness_parity.rs` wrappers.

interface Named {
    name: string,
}

test "builder_build_with_implementations_validates_then_freezes_atomically" {
    let person = reflect.class.builder("BuiltPerson");
    let address = reflect.class.builder("BuiltAddress");
    let person_pending = person.type();
    let address_pending = address.type();
    person.field("name", reflect.Type.of<string>());
    person.field("address", address_pending);
    address.field("owner", person_pending.optional());

    let incomplete = reflect.interface.implementation<Named>();
    let failure_code = {
        person.build(implementations = [incomplete]);
        "missing witness did not fail"
    } catch (e) {
        reflect.errors.CompilationError => e.diagnostics[0].code,
    };

    // Failed witness validation must not freeze any member of the group.
    address.field("zip", reflect.Type.of<int>());

    let complete = incomplete.field("name");
    let person_t = person.build(implementations = [complete]);
    let address_t = address.build();
    let person_again = person.build(implementations = [complete]);
    let frozen_message = {
        address.field("late", reflect.Type.of<bool>());
        "connected builder did not freeze"
    } catch (e) {
        reflect.errors.CompilationError => e.diagnostics[0].message,
    };

    assert.equal(failure_code, "E0001");
    assert.is_true(person_t.as_type().implements(reflect.Type.of<Named>()));
    assert.equal(person_pending.resolved(), person_t.as_type());
    assert.equal(address_pending.resolved(), address_t.as_type());
    assert.equal(person_again.as_type(), person_t.as_type());
    assert.is_true(frozen_message.ends_with("`BuiltAddress` is frozen and cannot be mutated"))
}

test "map_and_builder_witnessed_classes_are_definition_equal" {
    let witness = reflect.interface.implementation<Named>().field("name", class_field = "display_name");
    let map_t = reflect.class.new(
        "Person",
        { "display_name": reflect.Type.of<string>().meta(description = "public name") },
        implementations = [witness],
    );

    let builder = reflect.class.builder("Person");
    builder.field("display_name", reflect.Type.of<string>().meta(description = "public name"));
    let builder_t = builder.build(implementations = [witness]);

    assert.is_true(map_t.as_type() != builder_t.as_type());
    assert.equal(map_t.as_type().to_baml(), builder_t.as_type().to_baml());
    assert.is_true(map_t.as_type().implements(reflect.Type.of<Named>()));
    assert.is_true(builder_t.as_type().implements(reflect.Type.of<Named>()))
}

test "resolved_pending_from_frozen_group_is_accepted_by_another_builder" {
    let child = reflect.class.builder("ResolvedChild");
    let child_pending = child.type();
    child.field("value", reflect.Type.of<string>());
    let child_t = child.build();

    let parent = reflect.class.builder("Parent");
    parent.field("child", child_pending);
    let parent_t = parent.build();

    assert.equal(child_pending.resolved(), child_t.as_type());
    assert.equal(parent_t.fields()[0].type, child_t.as_type())
}
