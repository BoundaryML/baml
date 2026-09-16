// Registering interface witnesses on a runtime class (`reflect.class.new`,
// `reflect.class.builder`) is judged as one batch against the rules that
// already exist: a witness a static blanket already covers is an overlap, not
// a silent shadow. The published rules then live exactly as long as the class.

interface DirGate {}

interface DirPick {}

interface DirChoice<T> { type Item }

implements<T extends DirGate> DirPick for T {}

test "dynamic_registration_rejects_blanket_activated_by_same_batch" {
    let gate = reflect.interface.implementation<DirGate>();
    let pick = reflect.interface.implementation<DirPick>();
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
                reflect.interface.implementation<DirChoice<int, Item = int>>(),
                reflect.interface.implementation<DirChoice<int, Item = string>>(),
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
                reflect.interface.implementation<DirPick>(),
                reflect.interface.implementation<DirChoice<int, Item = int>>(),
                reflect.interface.implementation<DirChoice<string, Item = string>>(),
            ],
        )
        .as_type();
    assert.is_true(adapter.implements(reflect.Type.of<DirPick>()));
    assert.is_true(adapter.implements(reflect.Type.of<DirChoice<int, Item = int>>()));
    assert.is_true(adapter.implements(reflect.Type.of<DirChoice<string, Item = string>>()));
    assert.is_true(!adapter.implements(reflect.Type.of<DirGate>()));
}

test "dynamic_registration_rejected_batch_leaves_builder_retryable" {
    let builder = reflect.class.builder("Adapter");
    let pending = builder.type();
    let rejected = {
        builder.build(
            implementations = [
                reflect.interface.implementation<DirGate>(),
                reflect.interface.implementation<DirPick>(),
            ],
        );
        false
    } catch (e) {
        reflect.errors.CompilationError => {
            e.diagnostics[0].message.includes("overlapping implementations")
        },
    };
    assert.is_true(rejected);
    assert.is_true(pending.resolved() == null);
    let built = builder.build(implementations = [reflect.interface.implementation<DirPick>()]).as_type();
    assert.equal(pending.resolved(), built);
    assert.is_true(built.implements(reflect.Type.of<DirPick>()));
}

interface DirGreeter {
    function greet(self) -> string throws never { "hello" }
}

// Returns only an instance: the class and its witness rules stay reachable
// through the instance alone, never through a type value the caller holds.
function dir_greeter_instance() -> DirGreeter {
    let adapter = reflect.class.new(
        "Adapter",
        {},
        implementations = [reflect.interface.implementation<DirGreeter>()],
    );
    type Adapter = unreflect(adapter.as_type());
    let instance: Adapter = baml.json.from_string<Adapter>("{}");
    match (instance) {
        let greeter: DirGreeter => greeter,
        _ => throw "the witnessed class does not implement DirGreeter",
    }
}

test "dynamic_registration_rules_survive_collection_with_only_an_instance_rooted" {
    let greeter = dir_greeter_instance();
    baml.sys.collect_garbage();
    assert.equal(greeter.greet(), "hello");
}
