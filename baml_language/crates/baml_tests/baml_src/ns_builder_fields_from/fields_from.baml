// `reflect.class.Builder.fields_from` copies every field of a class, in
// order, with all of its metadata.

interface Named {
    name: string,
}

/// An agent step.
class Action {
    /// Which tool runs.
    tool: string @alias("tool_name") @description("The tool to call."),
    args: map<string, string>,
    trace_id: string? @skip,
    @@description("One step")
}

class Person {
    name: string,
    age: int,
    implements Named {}
}

/// Every field of `copy` matches the same field of `source`: name, type and
/// each metadata attribute.
function same_fields(copy: reflect.Class, source: reflect.Class) -> bool {
    let copied = copy.fields();
    let original = source.fields();
    if (copied.length() != original.length()) {
        return false;
    }
    for (let i = 0; i < original.length(); i += 1) {
        let a = copied[i];
        let b = original[i];
        let same_other = a.meta.other.length() == b.meta.other.length();
        for (let key in b.meta.other.keys()) {
            same_other = same_other && a.meta.other.get(key) == b.meta.other.get(key);
        }
        let same = a.name == b.name
            && a.type == b.type
            && a.meta.alias == b.meta.alias
            && a.meta.description == b.meta.description
            && a.meta.docstring == b.meta.docstring
            && a.meta.skip == b.meta.skip
            && same_other;
        if (!same) {
            return false;
        }
    }
    true
}

test "fields_from_copies_every_field_and_its_metadata" {
    let action = Action.type().as_class() ?? throw "missing Action";
    let copy = reflect.Class.builder("Action").fields_from(action).build();

    assert.is_true(same_fields(copy, action));
    let fields = copy.fields();
    assert.equal(fields[0].meta.description, "The tool to call.");
    assert.equal(fields[0].meta.alias, "tool_name");
    assert.equal(fields[0].meta.docstring, "Which tool runs.");
    assert.is_true(fields[2].meta.skip);
    // Only the fields: the class's own metadata stays behind.
    assert.equal(copy.metadata().description, null);
    assert.equal(copy.metadata().docstring, null)
}

test "fields_from_copy_renders_like_the_source_fields" {
    let source = reflect.Class.new(
        "Tagged",
        {
            "tag": string.type().meta(
                alias = "wire_tag",
                description = "dispatch key",
                docstring = "source docs",
                other = { "owner": "slice3" },
            ),
            "count": reflect.Type.of<int?>(),
        },
    );
    let copy = reflect.Class.builder("Tagged").fields_from(source).build();

    assert.equal(copy.to_baml(), source.to_baml());
    assert.is_true(same_fields(copy, source));
    assert.equal(copy.fields()[0].meta.other.get("owner"), "slice3")
}

test "fields_from_chains_with_field_and_build" {
    let action_class = reflect.Class.builder("ActionWithReason")
        .fields_from(Action.type().as_class() ?? throw "missing")
        .field("reason", string.type())
        .build();

    let names: string[] = [];
    for (let field in action_class.fields()) {
        names.push(field.name);
    }
    assert.equal(names.join("|"), "tool|args|trace_id|reason");
    assert.equal(action_class.fields()[3].type, string.type());
    assert.equal(action_class.fields()[0].meta.description, "The tool to call.")
}

test "fields_from_then_a_duplicate_field_throws" {
    let action = Action.type().as_class() ?? throw "missing";
    let builder = reflect.Class.builder("Duplicate").fields_from(action);
    let code = {
        builder.field("tool", int.type());
        "duplicate did not throw"
    } catch (e) {
        reflect.errors.CompilationError => e.diagnostics[0].code,
    };
    assert.equal(code, "E0012")
}

test "fields_from_a_colliding_class_throws_and_appends_nothing" {
    let action = Action.type().as_class() ?? throw "missing";
    let builder = reflect.Class.builder("Collides").field("args", int.type());
    let message = {
        builder.fields_from(action);
        "collision did not throw"
    } catch (e) {
        reflect.errors.CompilationError => e.diagnostics[0].message,
    };
    let built = builder.build();

    assert.equal(message, "duplicate field `Collides.args`");
    assert.equal(built.fields().length(), 1);
    assert.equal(built.fields()[0].type, int.type())
}

test "fields_from_on_a_frozen_builder_throws" {
    let action = Action.type().as_class() ?? throw "missing";
    let builder = reflect.Class.builder("Frozen");
    builder.build();
    let message = {
        builder.fields_from(action);
        "frozen builder accepted fields"
    } catch (e) {
        reflect.errors.CompilationError => e.diagnostics[0].message,
    };
    assert.is_true(message.ends_with("`Frozen` is frozen and cannot be mutated"))
}

test "fields_from_inside_a_recursive_builder_group" {
    let action = Action.type().as_class() ?? throw "missing";
    let step = reflect.Class.builder("Step");
    let plan = reflect.Class.builder("Plan");
    step.fields_from(action)
        .field("next", step.type().optional().meta(description = "The step after this one."))
        .field("plan", plan.type());
    plan.field("steps", step.type().array());

    let step_t = step.build();
    let plan_t = plan.build();
    let fields = step_t.fields();

    assert.equal(fields.length(), 5);
    assert.equal(fields[0].meta.description, "The tool to call.");
    assert.is_true(fields[2].meta.skip);
    assert.equal(fields[3].type, step_t.optional().as_type());
    assert.equal(fields[3].meta.description, "The step after this one.");
    assert.equal(fields[4].type, plan_t.as_type());
    assert.equal(plan_t.fields()[0].type, step_t.array().as_type())
}

test "fields_from_builds_with_an_explicit_implementation" {
    let person = Person.type().as_class() ?? throw "missing";
    let named = reflect.Type.of<Named>();

    // The interfaces are not copied: `Person` implements `Named`, the copy
    // does not until `build` is given the implementation.
    let plain = reflect.Class.builder("PlainPerson").fields_from(person).build();
    let employee = reflect.Class.builder("Employee")
        .fields_from(person)
        .field("team", string.type())
        .build(implementations = [reflect.Interface.implementation<Named>().field("name")]);

    assert.is_true(person.implements(named));
    assert.equal(plain.implements(named), false);
    assert.is_true(employee.implements(named))
}
