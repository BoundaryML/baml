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
    throw "with_types changed the exported type"
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
    throw "compiled wrapper did not return the mounted type"
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
