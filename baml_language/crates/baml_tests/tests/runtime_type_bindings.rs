//! Executable oracles for Package type views and scoped runtime
//! type bindings.

use baml_tests::baml_test;
use bex_engine::BexExternalValue;

const PATTERN_TWO_SOURCE: &str = r####"
interface PersonAnchor {
  name string
  email string
}

function TenantPersonType() -> reflect.class.Type {
  let anchor_impl = reflect.interface.implementation<PersonAnchor>()
    .field("name")
    .field("email")
  reflect.class.new("AcmePerson", {
    "name": type.of<string>(),
    "email": type.of<string>(),
    "favorite_editor": type.of<string>(),
  }, implementations = [anchor_impl])
}

function main() -> string throws unknown {
  let person_t = TenantPersonType()
  let app = reflect.Package.current().with_types({ "AcmePerson": person_t })
  let exported = app.get_class("root.AcmePerson") ?? throw "missing mounted class"
  if (exported.as_type() != person_t.as_type()) {
    throw "with_types changed the mint"
  }

  let pkg = reflect.Package.compile({ "tenant.baml": #"
function Run(document: string) -> app.AcmePerson {
  app.AcmePerson {
    name: "Ada",
    email: "ada@example.com",
    favorite_editor: document,
  }
}
"# }, packages = { "app": app })

  let extract = pkg.get_function<(string) -> PersonAnchor>("root.Run")
    ?? throw "missing root.Run"
  let person = extract(#"{"name":"Ada","email":"ada@example.com","favorite_editor":"vim"}"#)
  if (type.of_value(person) != person_t.as_type()) {
    throw "compiled wrapper did not return the mounted mint"
  }
  person.name + "|" + person.email
}
"####;

const TYPE_BINDING_SOURCE: &str = r####"
function main() -> bool throws unknown {
  let pkg = reflect.Package.compile({ "items.baml": #"
class Item { value string }
function Items() -> Item[] { [Item { value: "bound" }] }
  "# })
  let item_ct = pkg.get_class("root.Item") ?? throw "missing Item"
  let binding_evaluations: int = 0
  let operand = () -> {
    binding_evaluations += 1
    item_ct.as_type()
  }

  let escaped: unknown = {
    type T = unreflect(operand());
    if (type.of<T>() != item_ct.as_type()) {
      throw "type.of<T>() did not preserve the bound value"
    }
    let get_items = pkg.get_function<() -> T[]>("root.Items")
      ?? throw "missing root.Items"
    let items: T[] = get_items()
    let item: T = items[0]
    if (type.of_value(item) != type.of<T>()) {
      throw "typed result did not retain T"
    }
    item
  }

  binding_evaluations == 1
    && type.of_value(escaped) == item_ct.as_type()
}
"####;

#[tokio::test]
async fn scenario_four_pattern_two_mounts_a_runtime_type_as_a_static_name() {
    let output = baml_test!(PATTERN_TWO_SOURCE);
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("Ada|ada@example.com".into()))
    );
}

#[tokio::test]
async fn scoped_type_binding_evaluates_once_types_contracts_and_widens_on_escape() {
    let output = baml_test!(TYPE_BINDING_SOURCE);
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
async fn type_bindings_work_in_lambdas_and_nested_shadowing_uses_distinct_slots() {
    let output = baml_test!(
        r#"
function main() -> bool {
  let string_t = type.of<string>()
  let int_t = type.of<int>()
  let check = (bound: type) -> {
    type T = unreflect(bound);
    type.of<T>() == bound
  }

  type T = unreflect(string_t);
  let inner = {
    type T = unreflect(int_t);
    type.of<T>() == int_t
  }
  type.of<T>() == string_t && inner && check(int_t)
}
"#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
async fn scoped_type_argument_without_value_args_enforces_runtime_bounds() {
    let output = baml_test!(
        r#"
interface SomeInterface {
  label string
}

function needs_bound<A extends SomeInterface>() -> string {
  "ok"
}

function main() -> bool {
  let witness = reflect.interface.implementation<SomeInterface>().field("label")
  let conforming = reflect.class.new("Conforming", {
    "label": type.of<string>(),
  }, implementations = [witness])
  let nonconforming = reflect.class.new("Nonconforming", {
    "other": type.of<string>(),
  })

  let rejected = {
    type T = unreflect(nonconforming.as_type());
    let result = needs_bound<T>() catch (e) {
      baml.reflect.errors.CompilationError => e.diagnostics[0].code
    }
    result is string && result == "E0001"
  }
  let accepted = {
    type T = unreflect(conforming.as_type());
    needs_bound<T>() == "ok"
  }
  rejected && accepted
}
"#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
#[should_panic(expected = "runtime type bindings are only allowed inside")]
async fn top_level_runtime_type_binding_is_rejected() {
    let _ = baml_test!(
        r#"
type T = unreflect(type.of<string>())
function main() -> string { "unreachable" }
"#
    );
}

#[tokio::test]
#[should_panic(expected = "unresolved type: T")]
async fn type_binding_name_is_not_visible_outside_its_block() {
    let _ = baml_test!(
        r#"
function invalid() -> unknown {
  let t = type.of<string>()
  let escaped: unknown = {
    type T = unreflect(t);
    null
  }
  let invalid: T = escaped
  invalid
}

function main() -> null { null }
"#
    );
}

#[tokio::test]
async fn with_types_rejects_collisions_with_existing_exports() {
    let output = baml_test!(
        r#"
type ExistingAlias = string
function ExistingFunction() -> null { null }

function main() -> string throws unknown {
  let dynamic_t = reflect.class.new("Dynamic", { "value": type.of<string>() })
  let alias_collision = reflect.Package.current().with_types({ "ExistingAlias": dynamic_t }) catch (e) {
    baml.reflect.errors.CompilationError => e.diagnostics[0].code
  }
  let function_collision = reflect.Package.current().with_types({ "ExistingFunction": dynamic_t }) catch (e) {
    baml.reflect.errors.CompilationError => e.diagnostics[0].code
  }
  let first_view = reflect.Package.current().with_types({ "Mounted": dynamic_t })
  let view_collision = first_view.with_types({ "Mounted": dynamic_t }) catch (e) {
    baml.reflect.errors.CompilationError => e.diagnostics[0].code
  }
  let a = if alias_collision is string { alias_collision } else { "alias collision accepted" }
  let f = if function_collision is string { function_collision } else { "function collision accepted" }
  let v = if view_collision is string { view_collision } else { "view collision accepted" }
  a + "|" + f + "|" + v
}
"#
    );
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("E0011|E0011|E0011".into()))
    );
}

#[tokio::test]
async fn with_types_rejects_non_identifier_keys() {
    let output = baml_test!(
        r#"
function main() -> string throws unknown {
  let dynamic_t = reflect.class.new("Dynamic", { "value": type.of<string>() })
  let result = reflect.Package.current().with_types({ "not an identifier": dynamic_t }) catch (e) {
    baml.reflect.errors.CompilationError => e.diagnostics[0].code
  }
  if result is string { result } else { "invalid key was accepted" }
}
"#
    );
    assert_eq!(output.result, Ok(BexExternalValue::String("E0010".into())));
}

/// A scripted `ai.Client` that answers with a fixed payload, so the Agent loop
/// runs end to end without a network or an API key.
const PROBE_CLIENT: &str = r##"
client DefaultClient = openai.ResponsesClient.new(
    model = "gpt-4o-mini",
    api_key = "test-key",
    base_url = "http://localhost:1234",
);

class ProbeClient {
    reply: string,

    implements ai.Client {
        function id(self) -> string {
            "probe"
        }

        function invoke(self, input: ai.ModelTurnInput) -> ai.ModelTurn {
            let _ = input;
            ai.ModelTurn {
                content: [ai.content.Text { text: self.reply }],
                stop_reason: ai.content.StopReason.Complete,
                usage: null,
            }
        }
    }
}
"##;

/// B-1582 item 1: an interface-impl method reads its owner's type argument out
/// of the resolved impl frame, which carries realized types but no runtime
/// definition overlay. Anything that needs the *definition* — SAP, rendering,
/// reflection — used to fail on a type the caller could use fine.
#[tokio::test]
async fn interface_impl_methods_keep_runtime_type_definitions() {
    let output = baml_test!(
        r##"
        interface Parser<Out> {
            function parse_it(self, text: string) -> Out throws unknown
            function describe(self) -> string throws never
        }

        class Holder<T> {
            function new() -> Holder<T> throws never {
                Holder {}
            }

            implements Parser<T> {
                function parse_it(self, text: string) -> T throws unknown {
                    baml.sap.parse<T>(text)
                }

                function describe(self) -> string throws never {
                    type.of<T>().to_string()
                }
            }
        }

        function main() -> string throws unknown {
            let output_type = reflect.class.new("RuntimeOutput", {
                "name": type.of<string>(),
            }).as_type()
            type Out = unreflect(output_type)

            let holder = Holder<Out>.new()
            let parsed = holder.parse_it(#"{"name":"Pixel"}"#)
            `${holder.describe()}|${reflect.class.get_field<string>(parsed, "name")}`
        }
        "##
    );
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("RuntimeOutput|Pixel".into()))
    );
}

/// The ticket's Agent repro: `ai.Agent<Out>.run` is `implements Runner<Out>`, so
/// it took the same hole — a payload `baml.sap.parse<unreflect(t)>` handled fine
/// came back as `ai.errors.ParseFailed` through the runner.
#[tokio::test]
async fn agent_run_parses_a_reflected_output_type() {
    let source = format!(
        r##"
        {PROBE_CLIENT}

        function DynamicOutput<T>() -> T {{
            client: DefaultClient
            prompt: `${{ctx.output_format}}`
        }}

        function main() -> string throws unknown {{
            let output_type = reflect.class.new("RuntimeOutput", {{
                "name": type.of<string>(),
            }}).as_type()
            type Out = unreflect(output_type)

            // Control: the direct SAP call has always worked.
            let direct = baml.sap.parse<Out>(#"{{"name":"Pixel"}}"#)

            let run = ai.Agent<Out>.new(
                client = ProbeClient {{ reply: #"{{"name":"Pixel"}}"# }},
            ).run(DynamicOutput@spec<Out>())

            let direct_name = reflect.class.get_field<string>(direct, "name")
            let agent_name = reflect.class.get_field<string>(run.value, "name")
            `${{direct_name}}|${{agent_name}}`
        }}
        "##
    );
    let output = baml_test!(&source);
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("Pixel|Pixel".into()))
    );
}

/// B-1582 follow-up: definitions crossed dispatch, but *identity* did not.
/// The resolver realizes an impl frame off the receiver's `Self`, which carries
/// realized types only, so `type.of<T>()` in the body derived a fresh mint —
/// structurally the right type, `==`-wrong against the value the caller minted.
/// Every identity-keyed pattern (a registry, a stored type compared with `==`)
/// silently missed. Covered here: an implements-block method, an inherited
/// default method, and a second dispatch out of an impl body.
#[tokio::test]
async fn minted_type_identity_survives_interface_dispatch() {
    let output = baml_test!(
        r##"
        interface Probe<Out> {
            function same(self, t: type) -> bool throws never
            function same_from_default(self, t: type) -> bool throws never {
                type.of<Out>() == t
            }
        }

        interface Relay<Out> {
            function relay(self, t: type) -> bool throws unknown
        }

        class Holder<T> {
            function new() -> Holder<T> throws never {
                Holder {}
            }

            implements Probe<T> {
                function same(self, t: type) -> bool throws never {
                    type.of<T>() == t
                }
            }

            implements Relay<T> {
                function relay(self, t: type) -> bool throws unknown {
                    // Two-hop: the second interface operand is materialized
                    // inside this frame, so it has to carry the identity on.
                    Holder<T>.new().same(t)
                }
            }
        }

        function main() -> string throws unknown {
            let output_type = reflect.class.new("RuntimeOutput", {
                "name": type.of<string>(),
            }).as_type()
            type Out = unreflect(output_type)

            let holder = Holder<Out>.new()
            let impl_method = holder.same(output_type)
            let default_method = holder.same_from_default(output_type)
            let two_hop = holder.relay(output_type)
            `${impl_method}|${default_method}|${two_hop}`
        }
        "##
    );
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("true|true|true".into()))
    );
}

/// The pattern the identity is *for*: a registry keyed by the type value at the
/// call site, looked up by `==` inside the impl. The second entry is the
/// control — a separately minted class of the same shape and source name must
/// stay a miss, so a passing lookup cannot be `==` degenerating to `true`.
#[tokio::test]
async fn interface_impl_methods_look_up_a_type_keyed_registry() {
    let output = baml_test!(
        r##"
        class Entry {
            key: type,
            label: string,
        }

        interface Named<Out> {
            function label(self, first: Entry, second: Entry) -> string throws never
        }

        class Holder<T> {
            function new() -> Holder<T> throws never {
                Holder {}
            }

            implements Named<T> {
                function label(self, first: Entry, second: Entry) -> string throws never {
                    let wanted = type.of<T>()
                    if (first.key == wanted) {
                        first.label
                    } else if (second.key == wanted) {
                        second.label
                    } else {
                        "missing"
                    }
                }
            }
        }

        function main() -> string throws unknown {
            let first_type = reflect.class.new("Shape", {
                "name": type.of<string>(),
            }).as_type()
            let second_type = reflect.class.new("Shape", {
                "name": type.of<string>(),
            }).as_type()
            type First = unreflect(first_type)
            type Second = unreflect(second_type)

            let first_entry = Entry { key: first_type, label: "first" }
            let second_entry = Entry { key: second_type, label: "second" }

            let first_hit = Holder<First>.new().label(first_entry, second_entry)
            let second_hit = Holder<Second>.new().label(first_entry, second_entry)
            `${first_hit}|${second_hit}`
        }
        "##
    );
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("first|second".into()))
    );
}

/// Negative control: recovery reads the mint a definition records, so two
/// separate mints of the same shape stay distinct inside the impl, and a
/// statically instantiated type parameter keeps deriving its static digest —
/// the path is not taken at all when the interface operand carries no runtime
/// definitions.
#[tokio::test]
async fn dispatch_identity_separates_distinct_mints_and_leaves_static_generics_alone() {
    let output = baml_test!(
        r##"
        interface Probe<Out> {
            function same(self, t: type) -> bool throws never
        }

        class Holder<T> {
            function new() -> Holder<T> throws never {
                Holder {}
            }

            implements Probe<T> {
                function same(self, t: type) -> bool throws never {
                    type.of<T>() == t
                }
            }
        }

        function main() -> string throws unknown {
            let mine = reflect.class.new("Shape", {
                "name": type.of<string>(),
            }).as_type()
            let other = reflect.class.new("Shape", {
                "name": type.of<string>(),
            }).as_type()
            type Mine = unreflect(mine)

            let holder = Holder<Mine>.new()
            let own_mint = holder.same(mine)
            let foreign_mint = holder.same(other)

            let static_holder = Holder<string>.new()
            let static_match = static_holder.same(type.of<string>())
            let static_miss = static_holder.same(type.of<int>())
            `${own_mint}|${foreign_mint}|${static_match}|${static_miss}`
        }
        "##
    );
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("true|false|true|false".into()))
    );
}

/// A runtime *package*'s declarations do NOT keep identity across dispatch, and
/// that is deliberate. Recovery reads a definition back out of the interface
/// operand's overlay, which is keyed by qualified name — and a compiled
/// package's `Item` is plain `user.Item`, exactly like a static `Item` and like
/// every other package's `Item`. Answering from a name that several definitions
/// can spell would hand back a *wrong* mint (see the two regressions below), so
/// the recovery declines and the body re-derives a static value. Definitions
/// still travel (#4501), which is what `name()` pins: the impl body can still
/// read the type, it just does not hold the caller's identity token for it.
#[tokio::test]
async fn runtime_package_declarations_keep_definitions_but_not_identity() {
    let output = baml_test!(
        r##"
        interface Probe<Out> {
            function same(self, t: type) -> bool throws never
            function name(self) -> string throws never {
                type.of<Out>().to_string()
            }
        }

        class Holder<T> {
            function new() -> Holder<T> throws never {
                Holder {}
            }

            implements Probe<T> {
                function same(self, t: type) -> bool throws never {
                    type.of<T>() == t
                }
            }
        }

        function main() -> string throws unknown {
            let pkg = reflect.Package.compile({ "items.baml": #"
class Item { value string }
              "# })
            let item_type = (pkg.get_class("root.Item") ?? throw "missing Item").as_type()
            type Item = unreflect(item_type)

            let holder = Holder<Item>.new()
            `${holder.same(item_type)}|${holder.name()}`
        }
        "##
    );
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("false|Item".into()))
    );
}

/// A STATIC class must not be answered from an overlay that happens to carry a
/// runtime definition of the same name. `LoadType` staples the whole frame
/// overlay onto anything materialized in a frame that touched a runtime type,
/// so binding a compiled package's `Item` puts `user.Item` in this frame's
/// overlay — and the static `Holder<Item>` dispatch below then sees it. Reading
/// the overlay by plain name reported `type.of<T>() != type.of<Item>()` and
/// `== ` the *package's* mint, which no `$dyn`-named type can reproduce.
#[tokio::test]
async fn static_class_slots_are_not_answered_from_a_same_named_runtime_definition() {
    let output = baml_test!(
        r##"
        class Item {
            value: string,
        }

        interface Probe<Out> {
            function same(self, t: type) -> bool throws never
        }

        class Holder<T> {
            function new() -> Holder<T> throws never {
                Holder {}
            }

            implements Probe<T> {
                function same(self, t: type) -> bool throws never {
                    type.of<T>() == t
                }
            }
        }

        function main() -> string throws unknown {
            let pkg = reflect.Package.compile({ "items.baml": #"
class Item { value string }
              "# })
            let runtime_item = (pkg.get_class("root.Item") ?? throw "missing Item").as_type()
            // Binding it merges `user.Item` into this frame's overlay, which is
            // what every type materialized here from now on carries.
            type Shadow = unreflect(runtime_item)
            let _shadow_holder = Holder<Shadow>.new()

            let holder = Holder<Item>.new()
            `${holder.same(type.of<Item>())}|${holder.same(runtime_item)}`
        }
        "##
    );
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("true|false".into()))
    );
}

/// Two compiled packages that each declare `Item` produce two distinct mints
/// under one qualified name. An overlay keeps the first pointer it sees per
/// name, so a by-name recovery answered `Holder<B>` with A's mint — `==` true
/// against a type it is not, and false against the one it is. Declining is the
/// only honest answer available from a name alone.
#[tokio::test]
async fn same_named_declarations_from_two_packages_do_not_cross_match() {
    let output = baml_test!(
        r##"
        interface Probe<Out> {
            function same(self, t: type) -> bool throws never
        }

        class Holder<T> {
            function new() -> Holder<T> throws never {
                Holder {}
            }

            implements Probe<T> {
                function same(self, t: type) -> bool throws never {
                    type.of<T>() == t
                }
            }
        }

        function main() -> string throws unknown {
            let first = reflect.Package.compile({ "a.baml": #"
class Item { value string }
              "# })
            let second = reflect.Package.compile({ "b.baml": #"
class Item { value string }
              "# })
            let first_item = (first.get_class("root.Item") ?? throw "missing A").as_type()
            let second_item = (second.get_class("root.Item") ?? throw "missing B").as_type()
            type First = unreflect(first_item)
            type Second = unreflect(second_item)

            let _first_holder = Holder<First>.new()
            let holder = Holder<Second>.new()
            // Neither is claimed. The failure this guards is the SECOND value
            // being `true` — answering with a foreign package's identity.
            `${holder.same(second_item)}|${holder.same(first_item)}`
        }
        "##
    );
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("false|false".into()))
    );
}

/// Owner slots and method-level slots share one lane, and the owner slots are
/// recovered while the method slots arrive as operands. End-to-end cover for the
/// alignment the unit tests pin: a generic method on a generic impl, each slot a
/// different runtime mint.
#[tokio::test]
async fn dispatch_identity_covers_owner_and_method_slots_together() {
    let output = baml_test!(
        r##"
        interface Probe<Out> {
            function pair<M>(self, own: type, method: type) -> string throws never
        }

        class Holder<T> {
            function new() -> Holder<T> throws never {
                Holder {}
            }

            implements Probe<T> {
                function pair<M>(self, own: type, method: type) -> string throws never {
                    `${type.of<T>() == own}|${type.of<M>() == method}|${type.of<T>() == method}`
                }
            }
        }

        function main() -> string throws unknown {
            let owner_type = reflect.class.new("Owner", {
                "name": type.of<string>(),
            }).as_type()
            let method_type = reflect.class.new("Method", {
                "name": type.of<string>(),
            }).as_type()
            type Owner = unreflect(owner_type)
            type Method = unreflect(method_type)

            Holder<Owner>.new().pair<Method>(owner_type, method_type)
        }
        "##
    );
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("true|true|false".into()))
    );
}

/// A runtime *enum* in a class-level slot takes the same road as a runtime
/// class — the recovery has an enum arm, and nothing else exercises it.
#[tokio::test]
async fn dispatch_identity_covers_a_runtime_enum_slot() {
    let output = baml_test!(
        r##"
        interface Probe<Out> {
            function same(self, t: type) -> bool throws never
        }

        class Holder<T> {
            function new() -> Holder<T> throws never {
                Holder {}
            }

            implements Probe<T> {
                function same(self, t: type) -> bool throws never {
                    type.of<T>() == t
                }
            }
        }

        function main() -> string throws unknown {
            let choice = reflect.enum.new("Choice", ["FIRST", "SECOND"]).as_type()
            let other = reflect.enum.new("Choice", ["FIRST", "SECOND"]).as_type()
            type Choice = unreflect(choice)

            let holder = Holder<Choice>.new()
            `${holder.same(choice)}|${holder.same(other)}`
        }
        "##
    );
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("true|false".into()))
    );
}
