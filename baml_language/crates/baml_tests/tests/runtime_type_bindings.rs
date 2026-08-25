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
    "name": reflect.Type.of<string>(),
    "email": reflect.Type.of<string>(),
    "favorite_editor": reflect.Type.of<string>(),
  }, implementations = [anchor_impl])
}

function main() -> string throws unknown {
  let person_t = TenantPersonType()
  let app = reflect.Package.current().with_types({ "AcmePerson": person_t })
  let exported = app.get_class("root.AcmePerson") ?? throw "missing mounted class"
  if (exported.as_type() != person_t.as_type()) {
    throw "with_types changed the exported type"
  }

  let pkg = reflect.Package.compile({ "tenant.baml": `
function Run(document: string) -> app.AcmePerson {
  app.AcmePerson {
    name: "Ada",
    email: "ada@example.com",
    favorite_editor: document,
  }
}
` }, packages = { "app": app })

  let extract = pkg.get_function<(string) -> PersonAnchor>("root.Run")
    ?? throw "missing root.Run"
  let person = extract(`{"name":"Ada","email":"ada@example.com","favorite_editor":"vim"}`)
  if (reflect.Type.of_value(person) != person_t.as_type()) {
    throw "compiled wrapper did not return the mounted type"
  }
  person.name + "|" + person.email
}
"####;

const TYPE_BINDING_SOURCE: &str = r####"
function main() -> bool throws unknown {
  let pkg = reflect.Package.compile({ "items.baml": `
class Item { value string }
function Items() -> Item[] { [Item { value: "bound" }] }
  ` })
  let item_ct = pkg.get_class("root.Item") ?? throw "missing Item"
  let binding_evaluations: int = 0
  let operand = () -> {
    binding_evaluations += 1
    item_ct.as_type()
  }

  let escaped: unknown = {
    type T = unreflect(operand());
    if (reflect.Type.of<T>() != item_ct.as_type()) {
      throw "reflect.Type.of<T>() did not preserve the bound value"
    }
    let get_items = pkg.get_function<() -> T[]>("root.Items")
      ?? throw "missing root.Items"
    let items: T[] = get_items()
    let item: T = items[0]
    if (reflect.Type.of_value(item) != reflect.Type.of<T>()) {
      throw "typed result did not retain T"
    }
    item
  }

  binding_evaluations == 1
    && reflect.Type.of_value(escaped) == item_ct.as_type()
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
  let string_t = reflect.Type.of<string>()
  let int_t = reflect.Type.of<int>()
  let check = (bound: reflect.Type) -> {
    type T = unreflect(bound);
    reflect.Type.of<T>() == bound
  }

  type T = unreflect(string_t);
  let inner = {
    type T = unreflect(int_t);
    reflect.Type.of<T>() == int_t
  }
  reflect.Type.of<T>() == string_t && inner && check(int_t)
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
    "label": reflect.Type.of<string>(),
  }, implementations = [witness])
  let nonconforming = reflect.class.new("Nonconforming", {
    "other": reflect.Type.of<string>(),
  })

  let rejected = {
    type T = unreflect(nonconforming.as_type());
    let result = needs_bound<T>() catch (e) {
      reflect.errors.CompilationError => e.diagnostics[0].code
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
type T = unreflect(reflect.Type.of<string>())
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
  let t = reflect.Type.of<string>()
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
  let dynamic_t = reflect.class.new("Dynamic", { "value": reflect.Type.of<string>() })
  let alias_collision = reflect.Package.current().with_types({ "ExistingAlias": dynamic_t }) catch (e) {
    reflect.errors.CompilationError => e.diagnostics[0].code
  }
  let function_collision = reflect.Package.current().with_types({ "ExistingFunction": dynamic_t }) catch (e) {
    reflect.errors.CompilationError => e.diagnostics[0].code
  }
  let first_view = reflect.Package.current().with_types({ "Mounted": dynamic_t })
  let view_collision = first_view.with_types({ "Mounted": dynamic_t }) catch (e) {
    reflect.errors.CompilationError => e.diagnostics[0].code
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
  let dynamic_t = reflect.class.new("Dynamic", { "value": reflect.Type.of<string>() })
  let result = reflect.Package.current().with_types({ "not an identifier": dynamic_t }) catch (e) {
    reflect.errors.CompilationError => e.diagnostics[0].code
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

        function render(self, input: ai.ModelTurnInput) -> baml.http.Request {
            let _ = input;
            baml.http.Request { method: "POST", url: "https://probe.invalid", headers: {}, body: "{}" }
        }

        function invoke(self, input: ai.ModelTurnInput) -> ai.ModelTurn {
            let _ = input;
            ai.ModelTurn {
                calls: [],
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
                    reflect.Type.of<T>().to_string()
                }
            }
        }

        function main() -> string throws unknown {
            let output_type = reflect.class.new("RuntimeOutput", {
                "name": reflect.Type.of<string>(),
            }).as_type()
            type Out = unreflect(output_type)

            let holder = Holder<Out>.new()
            let parsed = holder.parse_it(`{"name":"Pixel"}`)
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
                "name": reflect.Type.of<string>(),
            }}).as_type()
            type Out = unreflect(output_type)

            // Control: the direct SAP call has always worked.
            let direct = baml.sap.parse<Out>(`{{"name":"Pixel"}}`)

            let run = ai.Agent.new(
                client = ProbeClient {{ reply: `{{"name":"Pixel"}}` }},
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
/// realized types only, so `reflect.Type.of<T>()` in the body derived a fresh mint —
/// structurally the right type, `==`-wrong against the value the caller minted.
/// Every identity-keyed pattern (a registry, a stored type compared with `==`)
/// silently missed. Covered here: an implements-block method, an inherited
/// default method, and a second dispatch out of an impl body.
#[tokio::test]
async fn minted_type_identity_survives_interface_dispatch() {
    let output = baml_test!(
        r##"
        interface Probe<Out> {
            function same(self, t: reflect.Type) -> bool throws never
            function same_from_default(self, t: reflect.Type) -> bool throws never {
                reflect.Type.of<Out>() == t
            }
        }

        interface Relay<Out> {
            function relay(self, t: reflect.Type) -> bool throws unknown
        }

        class Holder<T> {
            function new() -> Holder<T> throws never {
                Holder {}
            }

            implements Probe<T> {
                function same(self, t: reflect.Type) -> bool throws never {
                    reflect.Type.of<T>() == t
                }
            }

            implements Relay<T> {
                function relay(self, t: reflect.Type) -> bool throws unknown {
                    // Two-hop: the second interface operand is materialized
                    // inside this frame, so it has to carry the identity on.
                    Holder<T>.new().same(t)
                }
            }
        }

        function main() -> string throws unknown {
            let output_type = reflect.class.new("RuntimeOutput", {
                "name": reflect.Type.of<string>(),
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
            key: reflect.Type,
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
                    let wanted = reflect.Type.of<T>()
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
                "name": reflect.Type.of<string>(),
            }).as_type()
            let second_type = reflect.class.new("Shape", {
                "name": reflect.Type.of<string>(),
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
            function same(self, t: reflect.Type) -> bool throws never
        }

        class Holder<T> {
            function new() -> Holder<T> throws never {
                Holder {}
            }

            implements Probe<T> {
                function same(self, t: reflect.Type) -> bool throws never {
                    reflect.Type.of<T>() == t
                }
            }
        }

        function main() -> string throws unknown {
            let mine = reflect.class.new("Shape", {
                "name": reflect.Type.of<string>(),
            }).as_type()
            let other = reflect.class.new("Shape", {
                "name": reflect.Type.of<string>(),
            }).as_type()
            type Mine = unreflect(mine)

            let holder = Holder<Mine>.new()
            let own_mint = holder.same(mine)
            let foreign_mint = holder.same(other)

            let static_holder = Holder<string>.new()
            let static_match = static_holder.same(reflect.Type.of<string>())
            let static_miss = static_holder.same(reflect.Type.of<int>())
            `${own_mint}|${foreign_mint}|${static_match}|${static_miss}`
        }
        "##
    );
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("true|false|true|false".into()))
    );
}

/// A runtime *package*'s declarations keep their identity across dispatch.
/// Under name-keyed recovery this was deliberately FALSE — several definitions
/// can spell `user.Item`, so answering from the name risked a wrong identity
/// and recovery declined. A frame slot now carries the declaration's own head,
/// so `reflect.Type.of<T>()` in the impl body *is* the caller's type: there is no
/// separate identity token to lose, and nothing name-shaped to answer from.
/// `name()` still pins that the definition travels too.
#[tokio::test]
async fn runtime_package_declarations_keep_definitions_and_identity() {
    let output = baml_test!(
        r##"
        interface Probe<Out> {
            function same(self, t: reflect.Type) -> bool throws never
            function name(self) -> string throws never {
                reflect.Type.of<Out>().to_string()
            }
        }

        class Holder<T> {
            function new() -> Holder<T> throws never {
                Holder {}
            }

            implements Probe<T> {
                function same(self, t: reflect.Type) -> bool throws never {
                    reflect.Type.of<T>() == t
                }
            }
        }

        function main() -> string throws unknown {
            let pkg = reflect.Package.compile({ "items.baml": `
class Item { value string }
              ` })
            let item_type = (pkg.get_class("root.Item") ?? throw "missing Item").as_type()
            type Item = unreflect(item_type)

            let holder = Holder<Item>.new()
            `${holder.same(item_type)}|${holder.name()}`
        }
        "##
    );
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("true|Item".into()))
    );
}

/// A STATIC class must not be answered from an overlay that happens to carry a
/// runtime definition of the same name. `LoadType` staples the whole frame
/// overlay onto anything materialized in a frame that touched a runtime type,
/// so binding a compiled package's `Item` puts `user.Item` in this frame's
/// overlay — and the static `Holder<Item>` dispatch below then sees it. Reading
/// the overlay by plain name reported `reflect.Type.of<T>() != reflect.Type.of<Item>()` and
/// `==` the *package's* declaration, which no runtime-created type can
/// reproduce.
#[tokio::test]
async fn static_class_slots_are_not_answered_from_a_same_named_runtime_definition() {
    let output = baml_test!(
        r##"
        class Item {
            value: string,
        }

        interface Probe<Out> {
            function same(self, t: reflect.Type) -> bool throws never
        }

        class Holder<T> {
            function new() -> Holder<T> throws never {
                Holder {}
            }

            implements Probe<T> {
                function same(self, t: reflect.Type) -> bool throws never {
                    reflect.Type.of<T>() == t
                }
            }
        }

        function main() -> string throws unknown {
            let pkg = reflect.Package.compile({ "items.baml": `
class Item { value string }
              ` })
            let runtime_item = (pkg.get_class("root.Item") ?? throw "missing Item").as_type()
            // Binding it merges `user.Item` into this frame's overlay, which is
            // what every type materialized here from now on carries.
            type Shadow = unreflect(runtime_item)
            let _shadow_holder = Holder<Shadow>.new()

            let holder = Holder<Item>.new()
            `${holder.same(reflect.Type.of<Item>())}|${holder.same(runtime_item)}`
        }
        "##
    );
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("true|false".into()))
    );
}

/// Two compiled packages that each declare `Item` keep two identities. Each
/// grafted declaration is reminted with its own dynamic tag and the frame slot
/// carries its head, so nothing name-shaped exists to conflate them — the
/// shape that could only decline while both were spelled `user.Item` and a
/// name-keyed overlay kept the first pointer it saw per name.
#[tokio::test]
async fn same_named_declarations_from_two_packages_keep_separate_identities() {
    let output = baml_test!(
        r##"
        interface Probe<Out> {
            function same(self, t: reflect.Type) -> bool throws never
        }

        class Holder<T> {
            function new() -> Holder<T> throws never {
                Holder {}
            }

            implements Probe<T> {
                function same(self, t: reflect.Type) -> bool throws never {
                    reflect.Type.of<T>() == t
                }
            }
        }

        function main() -> string throws unknown {
            let first = reflect.Package.compile({ "a.baml": `
class Item { value string }
              ` })
            let second = reflect.Package.compile({ "b.baml": `
class Item { value string }
              ` })
            let first_item = (first.get_class("root.Item") ?? throw "missing A").as_type()
            let second_item = (second.get_class("root.Item") ?? throw "missing B").as_type()
            type First = unreflect(first_item)
            type Second = unreflect(second_item)

            let _first_holder = Holder<First>.new()
            let holder = Holder<Second>.new()
            // The holder's own declaration answers true — the slot carries its
            // head. The failure this guards is the SECOND value being `true`:
            // answering with a same-named foreign package's identity.
            `${holder.same(second_item)}|${holder.same(first_item)}`
        }
        "##
    );
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("true|false".into()))
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
            function pair<M>(self, own: reflect.Type, method: reflect.Type) -> string throws never
        }

        class Holder<T> {
            function new() -> Holder<T> throws never {
                Holder {}
            }

            implements Probe<T> {
                function pair<M>(self, own: reflect.Type, method: reflect.Type) -> string throws never {
                    `${reflect.Type.of<T>() == own}|${reflect.Type.of<M>() == method}|${reflect.Type.of<T>() == method}`
                }
            }
        }

        function main() -> string throws unknown {
            let owner_type = reflect.class.new("Owner", {
                "name": reflect.Type.of<string>(),
            }).as_type()
            let method_type = reflect.class.new("Method", {
                "name": reflect.Type.of<string>(),
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
            function same(self, t: reflect.Type) -> bool throws never
        }

        class Holder<T> {
            function new() -> Holder<T> throws never {
                Holder {}
            }

            implements Probe<T> {
                function same(self, t: reflect.Type) -> bool throws never {
                    reflect.Type.of<T>() == t
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

/// A runtime declaration's identity is its tag, never a spelling — so no
/// internal identity token exists to leak, and every surface that renders a
/// type name must show the name the source wrote. Pinned here at once:
/// `to_string`, `to_baml`, the LLM schema `ctx.output_format` builds, and a
/// compiler diagnostic.
#[tokio::test]
async fn a_package_declarations_identity_never_reaches_rendered_output() {
    let output = baml_test!(
        r##"
        client TestClient = openai.ResponsesClient.new(
            model = "gpt-4o-mini",
            api_key = "test-key",
            base_url = "http://localhost:1234",
        );

        function Render<T>() -> T {
            client: TestClient
            prompt: `${ctx.output_format}`
        }

        function main() -> string throws unknown {
            // `next` makes `Item` recursive, which forces the LLM schema to
            // hoist it under a *name* rather than inline its shape.
            let pkg = reflect.Package.compile({ "items.baml": `
class Item { value string, next Item? }
function Items() -> Item[] { [Item { value: "bound", next: null }] }
              ` })
            let item_type = (pkg.get_class("root.Item") ?? throw "missing Item").as_type()
            type Item = unreflect(item_type)

            // A diagnostic that has to name the type it rejected.
            let contract = pkg.get_function<() -> Item>("root.Items") catch (e) {
                reflect.errors.CompilationError => e.diagnostics[0].message,
                _ => "wrong error",
            }
            let diagnostic = if contract is string { contract } else { "no diagnostic" }

            // `~~` and not `|`: `to_baml()` and the LLM schema both spell a
            // union with `|`, so a `|` join could split *inside* a surface and
            // leave the assertions below silently comparing the wrong text.
            let schema = Render$render_prompt<Item[]>()
            `${item_type.to_string()}~~${item_type.to_baml()}~~${schema}~~${diagnostic}`
        }
        "##
    );
    let BexExternalValue::String(rendered) = output.result.expect("main must return") else {
        panic!("main returns a string");
    };
    assert!(
        !rendered.contains("$dyn"),
        "a runtime mint leaked into rendered output: {rendered}"
    );
    let mut parts = rendered.splitn(4, "~~");
    assert_eq!(parts.next(), Some("Item"));
    assert_eq!(
        parts.next(),
        Some("class Item {\n  value string\n  next Item?\n}")
    );
    let schema = parts.next().expect("schema");
    assert!(
        schema.contains("Item") && schema.contains("value"),
        "the LLM schema must describe the package class by its source name: {schema}"
    );
    let diagnostic = parts.next().expect("diagnostic");
    assert!(
        diagnostic.contains("Item"),
        "the diagnostic must name the package class by its source name: {diagnostic}"
    );
}

/// Binds `item_type` to a class named `Item`, compiled into a runtime package.
/// Both origins below create runtime declarations, so both must render them
/// the same way. Two fields, so a coercion failure is reported against the
/// class rather than being implied onto a lone field.
const ORIGIN_COMPILED_PACKAGE: &str = r##"
            let pkg = reflect.Package.compile({ "items.baml": `
class Item { value string, count int }
              ` })
            let item_type = (pkg.get_class("root.Item") ?? throw "missing Item").as_type()
"##;

/// The same `Item`, built by `reflect.class.new`.
const ORIGIN_CLASS_NEW: &str = r##"
            let item_type = reflect.class.new("Item", {
                "value": reflect.Type.of<string>(),
                "count": reflect.Type.of<int>(),
            }).as_type()
"##;

/// The two error surfaces that name a class in prose: schema-aligned parsing
/// (what an LLM's output is coerced through) and `baml.json` decoding.
fn error_surfaces_source(origin: &str) -> String {
    format!(
        r##"
        function main() -> string throws unknown {{
            {origin}
            type Item = unreflect(item_type)

            let sap = baml.sap.parse<Item>("null") catch (e) {{
                baml.errors.LlmClient => e.message,
                _ => "not an LlmClient error",
            }}
            let sap_message = if sap is string {{ sap }} else {{ "sap accepted a null" }}

            let decoded = baml.json.from_string<Item>("[1, 2]") catch (e) {{
                baml.json.JsonDecodeError => e.message,
                _ => "not a JsonDecodeError",
            }}
            let decode_message = if decoded is string {{ decoded }} else {{ "decode accepted a list" }}

            `${{sap_message}}~${{decode_message}}`
        }}
        "##
    )
}

/// Every surface that renders a runtime class must show the name its source
/// wrote, never an internal identity.
///
/// Schema-aligned parsing is the first of them. A runtime-*compiled* `Item`
/// renders package-qualified `user.Item` — exactly what a static `Item` prints
/// in the same error — while an anonymous `reflect.class.new` one renders the
/// bare `Item`: it has no package, so its display name is its only spelling.
#[tokio::test]
async fn a_coercion_error_names_a_runtime_class_as_its_source_spelled_it() {
    for (origin, expected) in [
        (ORIGIN_COMPILED_PACKAGE, "Expected user.Item"),
        (ORIGIN_CLASS_NEW, "Expected Item"),
    ] {
        let source = error_surfaces_source(origin);
        let output = baml_test!(&source);
        let BexExternalValue::String(rendered) = output.result.expect("main must return") else {
            panic!("main returns a string");
        };
        let (sap_message, _) = rendered.split_once('~').expect("both messages");
        assert!(
            sap_message.contains(expected),
            "a coercion error must name the class as its source spelled it \
             (want `{expected}`): {sap_message}"
        );
    }
}

/// `baml.json` decoding is the second. Two behaviors pinned at once: the
/// decoder resolves a runtime class through its head (a name lookup could not
/// see it at all), and its errors name the class as the source spelled it.
#[tokio::test]
async fn a_decode_error_names_a_runtime_class_as_its_source_spelled_it() {
    for origin in [ORIGIN_COMPILED_PACKAGE, ORIGIN_CLASS_NEW] {
        let source = error_surfaces_source(origin);
        let output = baml_test!(&source);
        let BexExternalValue::String(rendered) = output.result.expect("main must return") else {
            panic!("main returns a string");
        };
        let (_, decode_message) = rendered.split_once('~').expect("both messages");
        assert!(
            !decode_message.contains("$dyn"),
            "a runtime mint leaked into a decode error: {decode_message}"
        );
        assert!(
            decode_message.contains("expected JSON object for class `Item`"),
            "a decode error must name the class as its source spelled it: {decode_message}"
        );
    }
}

/// The fourth surface: a diagnostic from a *runtime* compile that has to name
/// a runtime declaration. The way one reaches a compile diagnostic is a
/// mounted type: `with_types` publishes an anonymous declaration under a mount
/// name, and a package compiled against it can then be wrong about it. The
/// message must name it the way the mount did — alias-qualified `app.Item`,
/// exactly as the failing source wrote it.
#[tokio::test]
async fn a_runtime_compile_diagnostic_names_a_mounted_runtime_class() {
    let output = baml_test!(
        r##"
        function main() -> string throws unknown {
            let item_type = reflect.class.new("Item", {
                "value": reflect.Type.of<string>(),
            }).as_type()
            let app = reflect.Package.current().with_types({ "Item": item_type })
            let compiled = reflect.Package.compile({ "wrong.baml": `
function Run() -> int { app.Item { value: "x" } }
              ` }, packages = { "app": app }) catch (e) {
                reflect.errors.CompilationError => e.diagnostics[0].message,
                _ => "not a CompilationError",
            }
            if compiled is string { compiled } else { "the wrong program compiled" }
        }
        "##
    );
    let BexExternalValue::String(diagnostic) = output.result.expect("main must return") else {
        panic!("main returns a string");
    };
    assert!(
        !diagnostic.contains("$dyn"),
        "a runtime mint leaked into a compile diagnostic: {diagnostic}"
    );
    assert_eq!(
        diagnostic.as_str(),
        "mismatched types: expected `int`, found `app.Item`"
    );
}

/// Identity by name is what a runtime type test reads, so two compiled packages
/// that each declare `Item` used to answer for each other: a value made by the
/// first package matched `is` against the second package's class. Nothing
/// reported an error — the branch simply ran on a value it was never given.
#[tokio::test]
async fn a_runtime_type_test_does_not_match_another_packages_same_named_class() {
    let output = baml_test!(
        r##"
        function main() -> string throws unknown {
            let first = reflect.Package.compile({ "a.baml": `
class Item { value string }
function Make() -> Item { Item { value: "a" } }
              ` })
            let second = reflect.Package.compile({ "b.baml": `
class Item { value string }
              ` })
            type First = unreflect((first.get_class("root.Item") ?? throw "missing A").as_type())
            type Second = unreflect((second.get_class("root.Item") ?? throw "missing B").as_type())

            let make = first.get_function<() -> First>("root.Make") ?? throw "missing root.Make"
            let value: unknown = make()
            let own = if value is First { "yes" } else { "no" }
            let foreign = if value is Second { "yes" } else { "no" }
            `${own}|${foreign}`
        }
        "##
    );
    assert_eq!(output.result, Ok(BexExternalValue::String("yes|no".into())));
}

/// The LLM schema `ctx.output_format` builds is assembled from a definition
/// overlay keyed by qualified name, so the same collision showed up there as a
/// prompt describing the *first* package's fields under the second package's
/// class. The model was then asked for a shape the caller never declared.
#[tokio::test]
async fn an_output_format_schema_describes_each_packages_own_class() {
    let output = baml_test!(
        r##"
        client TestClient = openai.ResponsesClient.new(
            model = "gpt-4o-mini",
            api_key = "test-key",
            base_url = "http://localhost:1234",
        );

        function Render<T>() -> T {
            client: TestClient
            prompt: `${ctx.output_format}`
        }

        function main() -> string throws unknown {
            // `next` makes each `Item` recursive, which forces the schema to
            // hoist it under a name rather than inline its shape.
            let first = reflect.Package.compile({ "a.baml": `
class Item { alpha string, next Item? }
              ` })
            let second = reflect.Package.compile({ "b.baml": `
class Item { beta int, next Item? }
              ` })
            type First = unreflect((first.get_class("root.Item") ?? throw "missing A").as_type())
            type Second = unreflect((second.get_class("root.Item") ?? throw "missing B").as_type())

            `${Render$render_prompt<First[]>()}~${Render$render_prompt<Second[]>()}`
        }
        "##
    );
    let BexExternalValue::String(rendered) = output.result.expect("main must return") else {
        panic!("main returns a string");
    };
    let (first, second) = rendered.split_once('~').expect("both schemas");
    assert!(
        !rendered.contains("$dyn"),
        "a runtime mint leaked into an LLM schema: {rendered}"
    );
    assert!(
        first.contains("alpha") && !first.contains("beta"),
        "the first package's schema must describe its own fields: {first}"
    );
    assert!(
        second.contains("beta") && !second.contains("alpha"),
        "the second package's schema must describe its own fields: {second}"
    );
}
