interface Gate {}

interface Pick {}

interface Choice<T> { type Item }

implements<T extends Gate> Pick for T {}

test "dynamic_registration_rejects_blanket_activated_by_same_batch" {
    let gate = reflect.interface.implementation<Gate>();
    let pick = reflect.interface.implementation<Pick>();
    for (let batch in [[gate, pick], [pick, gate]]) {
        let rejected = {
            reflect.class.new("Adapter", {}, implementations = batch);
            false
        } catch (e) {
            reflect.errors.CompilationError => {
                e.diagnostics[0].message.includes("overlapping implementations")
            },
        };
        assert.is_true(rejected);
    }
}

test "dynamic_registration_associated_outputs_do_not_disambiguate" {
    let rejected = {
        reflect.class.new(
            "Adapter",
            {},
            implementations = [
                reflect.interface.implementation<Choice<int, Item = int>>(),
                reflect.interface.implementation<Choice<int, Item = string>>(),
            ],
        );
        false
    } catch (e) {
        reflect.errors.CompilationError => {
            e.diagnostics[0].message.includes("overlapping implementations")
        },
    };
    assert.is_true(rejected);
}

test "dynamic_registration_distinct_inputs_and_inapplicable_blanket_are_allowed" {
    let adapter = reflect.class
        .new(
            "Adapter",
            {},
            implementations = [
                reflect.interface.implementation<Pick>(),
                reflect.interface.implementation<Choice<int, Item = int>>(),
                reflect.interface.implementation<Choice<string, Item = string>>(),
            ],
        )
        .as_type();
    assert.is_true(adapter.implements(reflect.Type.of<Pick>()));
    assert.is_true(adapter.implements(reflect.Type.of<Choice<int, Item = int>>()));
    assert.is_true(adapter.implements(reflect.Type.of<Choice<string, Item = string>>()));
    assert.is_true(!adapter.implements(reflect.Type.of<Gate>()));
}

test "dynamic_registration_rejected_builder_group_can_retry" {
    let parent = reflect.class.builder("Parent");
    let child = reflect.class.builder("Child");
    let parent_pending = parent.type();
    let child_pending = child.type();
    parent.field("child", child_pending);
    child.field("parent", parent_pending.optional());
    let rejected = {
        parent.build(
            implementations = [reflect.interface.implementation<Gate>(), reflect.interface.implementation<Pick>()],
        );
        false
    } catch (e) {
        reflect.errors.CompilationError => {
            e.diagnostics[0].message.includes("overlapping implementations")
        },
    };
    assert.is_true(rejected);
    assert.is_true(parent_pending.resolved() == null);
    assert.is_true(child_pending.resolved() == null);
    child.field("after_rejection", reflect.Type.of<int>());
    let parent_type = parent.build(implementations = [reflect.interface.implementation<Pick>()]).as_type();
    let child_type = child.build().as_type();
    assert.equal(parent_pending.resolved(), parent_type);
    assert.equal(child_pending.resolved(), child_type);
    assert.is_true(parent_type.implements(reflect.Type.of<Pick>()));
    assert.is_true(!parent_type.implements(reflect.Type.of<Gate>()));
}

interface Needs<T> requires Choice<T, Item = Self.Output> { type Output }

interface NeedsPick requires Pick {}

function rejects_requirement(choice: reflect.interface.Implementation) -> bool {
    {
        reflect.class.new(
            "NeedsAdapter",
            {},
            implementations = [reflect.interface.implementation<Needs<int, Output = string>>(), choice],
        );
        false
    } catch (e) {
        reflect.errors.CompilationError => e.diagnostics[0].message.includes("requires"),
    }
}

test "dynamic_registration_requires_exact_arguments_and_associated_projection" {
    assert.is_true(
        rejects_requirement(reflect.interface.implementation<Choice<string, Item = string>>()),
    );
    assert.is_true(rejects_requirement(reflect.interface.implementation<Choice<int, Item = int>>()));
    let accepted = reflect.class
        .new(
            "NeedsAdapter",
            {},
            implementations = [
                reflect.interface.implementation<Needs<int, Output = string>>(),
                reflect.interface.implementation<Choice<int, Item = string>>(),
            ],
        )
        .as_type();
    assert.is_true(accepted.implements(reflect.Type.of<Needs<int, Output = string>>()));
}

test "dynamic_registration_required_interface_can_come_from_blanket" {
    let accepted = reflect.class
        .new(
            "NeedsBlanket",
            {},
            implementations = [
                reflect.interface.implementation<NeedsPick>(),
                reflect.interface.implementation<Gate>(),
            ],
        )
        .as_type();
    assert.is_true(accepted.implements(reflect.Type.of<NeedsPick>()));
    assert.is_true(accepted.implements(reflect.Type.of<Pick>()));
}

test "dynamic_registration_failed_requirement_leaves_builder_retryable" {
    let builder = reflect.class.builder("NeedsAdapter");
    let pending = builder.type();
    let rejected = {
        builder.build(
            implementations = [
                reflect.interface.implementation<Needs<int, Output = string>>(),
                reflect.interface.implementation<Choice<string, Item = string>>(),
            ],
        );
        false
    } catch (e) {
        reflect.errors.CompilationError => e.diagnostics[0].message.includes("requires"),
    };
    assert.is_true(rejected);
    assert.is_true(pending.resolved() == null);
    let accepted = builder
        .build(
            implementations = [
                reflect.interface.implementation<Needs<int, Output = string>>(),
                reflect.interface.implementation<Choice<int, Item = string>>(),
            ],
        )
        .as_type();
    assert.equal(pending.resolved(), accepted);
    assert.is_true(accepted.implements(reflect.Type.of<Needs<int, Output = string>>()));
}
