//! Tests for BEP-057 associated types on interfaces.
//!
//! The suite covers declaration/binding syntax, default witnesses, projection
//! disambiguation, required-interface propagation, unions, destructuring, and
//! runtime dispatch through associated interface views.

use std::collections::HashSet;

use baml_compiler_diagnostics::Severity;
use baml_fmt::FormatOptions;
use baml_project::{ProjectDatabase, collect_diagnostics, testing::setup_test_db};
use baml_tests::{
    baml_test,
    engine::{OptLevel, compile_source_with_opt},
};
use bex_engine::BexExternalValue;
use bex_vm_types::Object;

fn collect_compile_errors(source: &str) -> Vec<String> {
    let db = setup_test_db(source);
    collect_compile_errors_from_db(&db)
}

fn collect_compile_errors_multi(files: &[(&str, &str)]) -> Vec<String> {
    let mut db = ProjectDatabase::new();
    db.set_project_root(std::path::Path::new("."));
    for (path, source) in files {
        db.add_file(*path, source);
    }
    collect_compile_errors_from_db(&db)
}

fn collect_compile_errors_from_db(db: &ProjectDatabase) -> Vec<String> {
    let project = db.get_project().expect("project must be set");
    let all_files = db.get_source_files();
    let user_file_ids: HashSet<_> = all_files.iter().map(|f| f.file_id(db)).collect();

    collect_diagnostics(db, project, &all_files)
        .into_iter()
        .filter(|d| matches!(d.severity, Severity::Error))
        .filter(|d| {
            d.primary_span()
                .map(|span| user_file_ids.contains(&span.file_id))
                .unwrap_or(false)
        })
        .map(|d| format!("[{}] {}", d.code(), d.message))
        .collect()
}

#[track_caller]
fn assert_zero_compile_errors(source: &str) {
    let errors = collect_compile_errors(source);
    assert!(
        errors.is_empty(),
        "expected zero compile errors, got:\n  {}",
        errors.join("\n  ")
    );
}

#[track_caller]
fn assert_zero_compile_errors_multi(files: &[(&str, &str)]) {
    let errors = collect_compile_errors_multi(files);
    assert!(
        errors.is_empty(),
        "expected zero compile errors, got:\n  {}",
        errors.join("\n  ")
    );
}

#[track_caller]
fn assert_compile_error_code(source: &str, code: &str) {
    let errors = collect_compile_errors(source);
    assert!(
        errors.iter().any(|error| error.contains(code)),
        "expected compile error containing `{code}`, got:\n  {}",
        errors.join("\n  ")
    );
}

#[track_caller]
fn assert_compile_error_code_multi(files: &[(&str, &str)], code: &str) {
    let errors = collect_compile_errors_multi(files);
    assert!(
        errors.iter().any(|error| error.contains(code)),
        "expected compile error containing `{code}`, got:\n  {}",
        errors.join("\n  ")
    );
}

#[track_caller]
fn assert_compile_error_contains(source: &str, needle: &str) {
    let errors = collect_compile_errors(source);
    assert!(
        errors.iter().any(|error| error.contains(needle)),
        "expected compile error containing `{needle}`, got:\n  {}",
        errors.join("\n  ")
    );
}

fn compiled_function_metadata(source: &str, display_name_suffix: &str) -> (Vec<String>, String) {
    let program = compile_source_with_opt(source, OptLevel::One);
    let matches: Vec<_> = program
        .function_indices
        .iter()
        .filter(|(name, _)| {
            name.strip_prefix("user.")
                .unwrap_or(name)
                .ends_with(display_name_suffix)
        })
        .collect();
    assert_eq!(
        matches.len(),
        1,
        "expected exactly one function ending with `{display_name_suffix}`, got: {:?}",
        matches
            .iter()
            .map(|(name, _)| name.as_str())
            .collect::<Vec<_>>()
    );

    let (name, idx) = matches[0];
    let Some(Object::Function(function)) = program.objects.get(*idx) else {
        panic!("`{name}` did not point at a function object");
    };

    (
        function
            .param_types
            .iter()
            .map(ToString::to_string)
            .collect(),
        function.return_type.to_string(),
    )
}

fn compiled_function_display_metadata(
    source: &str,
    display_name_suffix: &str,
) -> (Vec<String>, Vec<String>, String) {
    let program = compile_source_with_opt(source, OptLevel::One);
    let matches: Vec<_> = program
        .function_indices
        .iter()
        .filter(|(name, _)| {
            name.strip_prefix("user.")
                .unwrap_or(name)
                .ends_with(display_name_suffix)
        })
        .collect();
    assert_eq!(
        matches.len(),
        1,
        "expected exactly one function ending with `{display_name_suffix}`, got: {:?}",
        matches
            .iter()
            .map(|(name, _)| name.as_str())
            .collect::<Vec<_>>()
    );

    let (name, idx) = matches[0];
    let Some(Object::Function(function)) = program.objects.get(*idx) else {
        panic!("`{name}` did not point at a function object");
    };

    (
        function.display_type_params.clone(),
        function.display_param_types.clone(),
        function.display_return_type.clone(),
    )
}

#[test]
fn associated_type_declaration_forms_compile() {
    assert_zero_compile_errors(
        r#"
        interface Named {
            name: string
        }

        class Label {
            name: string
            implements Named {}
        }

        class City {
            id: int
        }

        class Road {
            name: string
            implements Named {}
        }

        interface Graph {
            type Node
            type Edge extends Named
            type Weight = int
            type LabelType extends Named = Label

            function neighbors(self, node: Self.Node) -> Self.Node[] throws never
            function edge_label(self, edge: Self.Edge) -> Self.LabelType throws never
            function weight(self, edge: Self.Edge) -> Self.Weight throws never
        }

        class CityMap {
            cities: City[]

            implements Graph {
                type Node = City
                type Edge = Road

                function neighbors(self, node: City) -> City[] {
                    return self.cities
                }

                function edge_label(self, edge: Road) -> Label {
                    return Label { name: edge.name }
                }

                function weight(self, edge: Road) -> int {
                    return 1
                }
            }
        }
        "#,
    );
}

#[test]
fn implementor_self_coerces_to_interface_with_associated_bindings_in_return_context() {
    assert_zero_compile_errors(
        r#"
        class Done {}

        interface Iterable {
            type Item
            type Error = never

            function iter(self) -> Iterator<Item = Self.Item, Error = Self.Error> throws never
        }

        interface Iterator requires Iterable<Item = Self.Item, Error = Self.Error> {
            type Item
            type Error = never

            function next(self) -> Self.Item | Done throws Self.Error
        }

        class ArrayIterator<T> {
            values: T[]
            idx: int

            implements Iterable {
                type Item = T
                type Error = never

                function iter(self) -> Iterator<Item = T, Error = never> throws never {
                    return self
                }
            }

            implements Iterator {
                type Item = T
                type Error = never

                function next(self) -> T | Done throws never {
                    match (self.values.at(self.idx)) {
                        null => Done {},
                        let value: T => {
                            self.idx += 1
                            value
                        },
                    }
                }
            }
        }
        "#,
    );
}

#[tokio::test]
async fn associated_type_projection_in_throws_position_runs() {
    let output = baml_test!(
        r#"
        class Boom {
            message: string
        }

        interface Fallible {
            type Error

            function value(self) -> int throws Self.Error

            function value_plus_one(self) -> int throws Self.Error {
                return self.value() + 1
            }
        }

        class AlwaysFails {
            implements Fallible {
                type Error = Boom

                function value(self) -> int throws Boom {
                    throw Boom { message: "boom" }
                }
            }
        }

        function call_value<F extends Fallible>(value: F) -> int throws F.Error {
            return value.value()
        }

        function main() -> string {
            let default_result = AlwaysFails {}.value_plus_one() catch (e) {
                Boom => "default:" + e.message
            }
            let generic_result = call_value(AlwaysFails {}) catch (e) {
                Boom => "generic:" + e.message
            }
            return default_result + "|" + generic_result
        }
        "#
    );

    assert_eq!(
        output.result.unwrap(),
        BexExternalValue::String("default:boom|generic:boom".into())
    );
}

#[test]
fn default_interface_method_can_thread_associated_error_through_callback() {
    assert_zero_compile_errors(
        r#"
        class Done {}

        interface Iterator {
            type Item
            type Error = never

            function next(self) -> Self.Item | Done throws Self.Error

            function map<R, E2>(self, f: (Self.Item) -> R throws E2) -> Mapper<Self.Item, R, Self.Error, E2> throws never {
                return Mapper<Self.Item, R, Self.Error, E2> { inner: self, f: f }
            }
        }

        class Mapper<T, R, E, E2> {
            inner: Iterator<Item = T, Error = E>
            f: (T) -> R throws E2

            implements Iterator {
                type Item = R
                type Error = E | E2

                function next(self) -> R | Done throws E | E2 {
                    match (self.inner.next()) {
                        Done => Done {},
                        let value: T => self.f(value),
                    }
                }
            }
        }
        "#,
    );
}

#[test]
fn associated_type_adapter_class_with_never_error_coerces_to_iterator() {
    assert_zero_compile_errors(
        r#"
        class Done {}

        interface Iterable {
            type Item
            type Error = never

            function iter(self) -> Iterator<Item = Self.Item, Error = Self.Error> throws never
        }

        interface Iterator requires Iterable<Item = Self.Item, Error = Self.Error> {
            type Item
            type Error = never

            function next(self) -> Self.Item | Done throws Self.Error
        }

        class Map<T, R, E, E2> {
            iter: Iterator<Item = T, Error = E>
            f: (T) -> R throws E2

            implements Iterable {
                type Item = R
                type Error = E | E2

                function iter(self) -> Iterator<Item = R, Error = E | E2> throws never {
                    self
                }
            }

            implements Iterator {
                type Item = R
                type Error = E | E2

                function next(self) -> R | Done throws E | E2 {
                    Done {}
                }
            }
        }

        function use_map() -> int {
            let source: Iterator<Item = int, Error = never> = Map<int, int, never, never> {
                iter: Empty {},
                f: (x: int) -> int { x },
            }
            let mapped: Iterator<Item = int, Error = never> = Map<int, int, never, never> {
                iter: source,
                f: (x: int) -> int { x * 3 },
            }
            1
        }

        class Empty {
            implements Iterable {
                type Item = int
                type Error = never

                function iter(self) -> Iterator<Item = int, Error = never> throws never {
                    self
                }
            }

            implements Iterator {
                type Item = int
                type Error = never

                function next(self) -> int | Done throws never {
                    Done {}
                }
            }
        }
        "#,
    );
}

#[test]
fn default_iterator_adapter_method_returns_adapter_with_symbolic_error_projection() {
    assert_zero_compile_errors(
        r#"
        class Done {}

        interface Iterable {
            type Item
            type Error = never

            function iter(self) -> Iterator<Item = Self.Item, Error = Self.Error> throws never
        }

        interface Iterator requires Iterable<Item = Self.Item, Error = Self.Error> {
            type Item
            type Error = never

            function next(self) -> Self.Item | Done throws Self.Error

            function map<R, E2>(self, f: (Self.Item) -> R throws E2) -> Iterator<Item = R, Error = Self.Error | E2> throws never {
                Map<Self.Item, R, Self.Error, E2> { iter: self, f: f }
            }
        }

        class Map<T, R, E, E2> {
            iter: Iterator<Item = T, Error = E>
            f: (T) -> R throws E2

            implements Iterable {
                type Item = R
                type Error = E | E2

                function iter(self) -> Iterator<Item = R, Error = E | E2> throws never {
                    self
                }
            }

            implements Iterator {
                type Item = R
                type Error = E | E2

                function next(self) -> R | Done throws E | E2 {
                    Done {}
                }
            }
        }
        "#,
    );
}

#[test]
fn vm_metadata_resolves_concrete_associated_type_projection_return() {
    let (_params, return_type) = compiled_function_metadata(
        r#"
        interface PublicIdentity {
            type Key
            key: Self.Key
        }

        class AccountRecord {
            public_key: string

            implements PublicIdentity {
                type Key = string
                key as public_key
            }
        }

        function get_public_key(account: AccountRecord) -> (AccountRecord as PublicIdentity).Key {
            return account.as<PublicIdentity<Key = string>>.key
        }
        "#,
        "get_public_key",
    );

    assert_eq!(return_type, "string");
}

#[test]
fn vm_metadata_resolves_self_associated_type_return_in_implements_method() {
    let (_params, return_type) = compiled_function_metadata(
        r#"
        interface Repository {
            type Record
            function find(self) -> Self.Record throws never
        }

        class UserRecord {
            name: string
        }

        class UserRepository {
            value: UserRecord

            implements Repository {
                type Record = UserRecord

                function find(self) -> Self.Record {
                    return self.value
                }
            }
        }
        "#,
        "UserRepository.Repository.find",
    );

    assert_eq!(return_type, "UserRecord");
}

#[test]
fn vm_metadata_preserves_unresolved_generic_associated_projection_symbolically() {
    let (params, return_type) = compiled_function_metadata(
        r#"
        interface BoxLike {
            type Item
            function get(self) -> Self.Item throws never
        }

        function read_item<T extends BoxLike>(box: T) -> T.Item {
            return box.get()
        }
        "#,
        "read_item",
    );

    // `T` and its associated projection cannot be resolved statically here, but they
    // are *not* erased: `RuntimeTy` carries the type variable and the symbolic
    // projection so the runtime can resolve them from the receiver's actual type. The
    // projection is carried in its resolved form `(T as BoxLike).Item` — the declaring
    // interface is determined at lowering, which is strictly more precise than the
    // bare `T.Item` for runtime resolution.
    assert_eq!(params, vec!["T"]);
    assert_eq!(return_type, "(T as BoxLike).Item");
}

#[test]
fn vm_metadata_displays_interface_default_method_self_type() {
    let (generic_params, params, return_type) = compiled_function_display_metadata(
        r#"
        interface Described<T> {
            function label(self) -> string throws never

            function describe(self) -> string throws never {
                return self.label()
            }
        }

        class Widget {
            name: string

            implements Described<string> {
                function label(self) -> string {
                    return self.name
                }
            }
        }
        "#,
        "Described.describe",
    );

    assert_eq!(generic_params, vec!["T"]);
    assert_eq!(params, vec!["Described<T>"]);
    assert_eq!(return_type, "string");
}

#[test]
fn associated_type_bindings_substitute_inside_implements_blocks() {
    assert_zero_compile_errors(
        r#"
        interface Stack {
            type Item

            function push(self, value: Self.Item) -> null throws never
            function peek(self) -> Self.Item? throws never
            function pair(self, value: Self.Item) -> Self.Item[] throws never {
                return [value, value]
            }
        }

        class IntStack {
            implements Stack {
                type Item = int

                function push(self, value: Self.Item) -> null {
                    return null
                }

                function peek(self) -> Self.Item? {
                    return null
                }
            }
        }

        function top(stack: IntStack) -> IntStack.Item? {
            return stack.peek()
        }
        "#,
    );
}

#[test]
fn default_method_may_return_self_call_yielding_associated_type() {
    // A default method whose body returns the result of a `self`-method that
    // yields the associated type must type-check: the declared return
    // (`Self.Item?`) and the body (`self.next()` → `Self.Item?`) must produce
    // the same associated-type projection. Both go through the interface's own
    // `Item` binding, which projects onto the rigid `Self` (not the interface
    // existential) — matching how the `self.next()` call resolves it. This is
    // the lazy-cursor / iterator delegating-default-method pattern.
    assert_zero_compile_errors(
        r#"
        interface It {
            type Item
            function next(self) -> Self.Item? throws never
            function firstOrNull(self) -> Self.Item? throws never {
                return self.next()
            }
        }
        class IntCursor {
            value int
            implements It {
                type Item = int
                function next(self) -> Self.Item? { return self.value }
            }
        }
        "#,
    );
}

#[tokio::test]
async fn default_method_self_call_yielding_associated_type_runs() {
    let output = baml_test!(
        r#"
        interface Describable {
            type Output

            function value(self) -> Self.Output throws never

            function describe(self) -> Self.Output throws never {
                return self.value()
            }
        }

        class Ticket {
            id: string

            implements Describable {
                type Output = string

                function value(self) -> Self.Output {
                    return self.id
                }
            }
        }

        function main() -> string {
            let ticket = Ticket { id: "A-100" }
            return ticket.describe()
        }
        "#
    );

    assert_eq!(
        output.result.unwrap(),
        BexExternalValue::String("A-100".into())
    );
}

#[test]
fn class_inherent_method_does_not_satisfy_abstract_associated_type_method() {
    // `Ticket`'s `value` is an inherent method (outside the `implements Describable`
    // block, which binds only `type Output`), so it does NOT satisfy the abstract
    // `Describable.value` (BEP-044: only `implements`-block members satisfy a
    // requirement) → E0113. (Previously the inherent method was wrongly accepted,
    // and the `describe` default's `self.value()` then `UnresolvedVirtualCall`-ed
    // at runtime — flakily, via M3 registry-order nondeterminism.)
    assert_compile_error_code(
        r#"
        interface Describable {
            type Output

            function value(self) -> Self.Output throws never

            function describe(self) -> Self.Output throws never {
                return self.value()
            }
        }

        class Ticket {
            id: string

            implements Describable {
                type Output = string
            }

            function value(self) -> Self.Output {
                return self.id
            }
        }
        "#,
        "E0113",
    );
}

#[test]
fn inherited_scalar_default_method_delegating_to_self_compiles() {
    // The inherited sibling of
    // `default_method_may_return_self_call_yielding_associated_type`: a child
    // interface (`Cursor requires It`) whose default method returns the inherited
    // associated type in a scalar/optional position (`Self.Item?`) and delegates
    // through `self.next()`. The inherited `Item` projects onto the rigid `Self`,
    // matching how `self.next()` resolves it, so the declared return and the body
    // agree. Distinct from `required_parent_associated_type_threads_into_child_interface`,
    // which uses a required child method implemented with a concrete `int[]` and so
    // never reconciles the projection symbolically inside a default body.
    assert_zero_compile_errors(
        r#"
        interface It {
            type Item
            function next(self) -> Self.Item? throws never
        }
        interface Cursor requires It {
            function peek(self) -> Self.Item? throws never {
                return self.next()
            }
        }
        class IntCursor {
            value int
            implements It {
                type Item = int
                function next(self) -> Self.Item? { return self.value }
            }
            implements Cursor {}
        }
        "#,
    );
}

#[test]
fn blanket_impl_binds_associated_type_in_default_body() {
    // A blanket out-of-body impl (`implements<T> Items for Box<T>`, so `Self = Box<T>`
    // and `Item = T`) whose interface has a default method returning `Self.Item[]` by
    // delegating through `self.items()`. Exercises associated-type binding through a
    // constructed (generic) `Self` in a default body — distinct from the existing
    // blanket tests, which bind associated types but have no `self`-delegating default.
    assert_zero_compile_errors(
        r#"
        interface Items {
            type Item
            function items(self) -> Self.Item[] throws never
            function firstItems(self) -> Self.Item[] throws never {
                return self.items()
            }
        }
        class Box<T> {
            values: T[]
        }
        implements<T> Items for Box<T> {
            type Item = T
            function items(self) -> Self.Item[] { return self.values }
        }
        "#,
    );
}

#[test]
fn fully_bound_associated_type_interface_values_expose_projected_methods() {
    assert_zero_compile_errors(
        r#"
        interface Iterator {
            type Item

            function next(self) -> Self.Item? throws never
            function size(self) -> int throws never
        }

        class IntIterator {
            implements Iterator {
                type Item = int

                function next(self) -> int? {
                    return null
                }

                function size(self) -> int {
                    return 0
                }
            }
        }

        function consume(it: Iterator<Item = int>) -> int? {
            let count = it.size()
            return it.next()
        }

        function main() -> int? {
            let it: Iterator<Item = int> = IntIterator {}
            return consume(it)
        }
        "#,
    );
}

#[test]
fn concrete_class_must_match_interface_associated_type_binding() {
    assert_compile_error_code(
        r#"
        interface Iterator {
            type Item

            function next(self) -> Self.Item? throws never
        }

        class IntIterator {
            implements Iterator {
                type Item = int

                function next(self) -> int? {
                    return null
                }
            }
        }

        function bad() -> string? {
            let it: Iterator<Item = string> = IntIterator {}
            return it.next()
        }
        "#,
        "E0001",
    );
}

#[test]
fn generic_constructor_result_checked_against_interface_associated_binding() {
    let errors = collect_compile_errors(
        r#"
        interface Value {
            type Item

            function get(self) -> Self.Item throws never
        }

        class Box<T> {
            value: T

            function new(value: T) -> Box<T> {
                return Box<T> { value: value }
            }

            implements Value {
                type Item = T

                function get(self) -> T {
                    return self.value
                }
            }
        }

        function bad() -> string {
            let n: int = 1
            let value: Value<Item = string> = Box.new(n)
            return value.get()
        }
        "#,
    );
    assert!(
        errors
            .iter()
            .any(|error| { error.contains("Value<Item = string>") && error.contains("Box<int>") }),
        "expected concrete associated-binding mismatch, got:\n  {}",
        errors.join("\n  ")
    );
}

#[test]
fn generic_bound_enforces_associated_type_binding() {
    assert_compile_error_code(
        r#"
        interface Iterator {
            type Item

            function next(self) -> Self.Item? throws never
        }

        class IntIterator {
            implements Iterator {
                type Item = int

                function next(self) -> int? {
                    return null
                }
            }
        }

        function take_string<I extends Iterator<Item = string>>(it: I) -> string? {
            return it.next()
        }

        function bad(it: IntIterator) -> string? {
            return take_string<IntIterator>(it)
        }
        "#,
        "E0001",
    );
}

#[test]
fn as_upcast_enforces_associated_type_binding() {
    assert_compile_error_code(
        r#"
        interface Iterator {
            type Item

            function next(self) -> Self.Item? throws never
        }

        class IntIterator {
            implements Iterator {
                type Item = int

                function next(self) -> int? {
                    return null
                }
            }
        }

        function bad(it: IntIterator) -> string? {
            return it.as<Iterator<Item = string>>.next()
        }
        "#,
        "E0001",
    );
}

#[test]
fn associated_type_projection_from_generic_interface_bound_compiles() {
    assert_zero_compile_errors(
        r#"
        interface Parser {
            type Output

            function parse(self, input: string) -> Self.Output throws never
        }

        class IntParser {
            implements Parser {
                type Output = int

                function parse(self, input: string) -> int {
                    return 42
                }
            }
        }

        function parse_one<P extends Parser>(parser: P, input: string) -> P.Output {
            return parser.parse(input)
        }

        function main() -> int {
            return parse_one<IntParser>(IntParser {}, "42")
        }
        "#,
    );
}

#[test]
fn associated_type_binding_in_generic_bound_concretizes_projection() {
    assert_zero_compile_errors(
        r#"
        interface Parser {
            type Output

            function parse(self) -> Self.Output throws never
        }

        class ConstantParser<T> {
            value: T
        }

        implements<T> Parser for ConstantParser<T> {
            type Output = T

            function parse(self) -> T {
                return self.value
            }
        }

        function parse_one<P extends Parser>(parser: P) -> P.Output {
            return parser.parse()
        }

        function parse_known_int<P extends Parser<Output = int>>(parser: P) -> int {
            return parse_one(parser)
        }

        function demo_int(parser: ConstantParser<int>) -> int {
            return parse_one(parser)
        }
        "#,
    );
}

#[tokio::test]
async fn blanket_impl_self_associated_projection_uses_bounded_typevar() {
    let output = baml_test!(
        r#"
        interface Source {
            type Item
            name: string
            function get(self) -> Self.Item throws never
        }

        class TextSource {
            name: string
            text: string

            implements Source {
                type Item = string

                function get(self) -> Self.Item {
                    return self.text
                }
            }
        }

        interface Renderable {
            type Output
            function render(self) -> Self.Output throws never
        }

        class Wrapped<T extends Source<Item = string>> {
            inner: T
        }

        implements<T extends Source<Item = string>> Renderable for Wrapped<T> {
            type Output = T.Item

            function render(self) -> Self.Output {
                return self.inner.get()
            }
        }

        function take_source<T extends Source<Item = string>>(source: T) -> T.Item {
            return source.get()
        }

        function main() -> string {
            let source = TextSource { name: "sample", text: "ok" }
            let wrapped = Wrapped<TextSource> { inner: source }

            return take_source(source) + ":" + wrapped.as<Renderable<Output = string>>.render()
        }
        "#
    );

    assert_eq!(
        output.result.unwrap(),
        BexExternalValue::String("ok:ok".into())
    );
}

#[tokio::test]
async fn blanket_impl_concrete_projection_return_resolves_at_callsite() {
    let output = baml_test!(
        r#"
        interface Source {
            type Item
            function get(self) -> Self.Item throws never
        }

        class TextSource {
            text: string

            implements Source {
                type Item = string

                function get(self) -> Self.Item {
                    return self.text
                }
            }
        }

        interface WrapperView {
            type Output
            function output(self) -> Self.Output throws never
        }

        class Wrapped<S extends Source<Item = string>> {
            inner: S
        }

        implements<S extends Source<Item = string>> WrapperView for Wrapped<S> {
            type Output = S.Item

            function output(self) -> Self.Output {
                return self.inner.get()
            }
        }

        function main() -> string {
            let wrapped = Wrapped<TextSource> { inner: TextSource { text: "hello" } }
            return wrapped.output()
        }
        "#
    );

    assert_eq!(
        output.result.unwrap(),
        BexExternalValue::String("hello".into())
    );
}

#[tokio::test]
async fn upcast_of_bounded_typevar_preserves_associated_bindings() {
    let output = baml_test!(
        r#"
        interface HasKey {
            type Key
            key: Self.Key
        }

        interface Named {
            name: string
        }

        interface Entity requires HasKey<Key = string>, Named {
        }

        class User {
            id: string
            name: string

            implements HasKey {
                type Key = string
                key as id
            }

            implements Named {
            }

            implements Entity {
            }
        }

        class EntityBox<T extends Entity> {
            value: T
        }

        interface Summarizes {
            type Key
            function key(self) -> Self.Key throws never
            function summary(self) -> string throws never
        }

        implements<T extends Entity> Summarizes for EntityBox<T> {
            type Key = (T as HasKey).Key

            function key(self) -> Self.Key {
                return self.value.as<HasKey<Key = string>>.key
            }

            function summary(self) -> string {
                return self.value.name + ":" + self.key()
            }
        }

        function entity_key<T extends Entity>(value: T) -> (T as HasKey).Key {
            return value.as<HasKey<Key = string>>.key
        }

        function main() -> string {
            let user = User { id: "u1", name: "Ada" }
            let boxed = EntityBox<User> { value: user }
            return boxed.summary() + "|" + entity_key(user)
        }
        "#
    );

    assert_eq!(
        output.result.unwrap(),
        BexExternalValue::String("Ada:u1|u1".into())
    );
}

#[test]
fn required_parent_associated_type_threads_into_child_interface() {
    assert_zero_compile_errors(
        r#"
        interface Iterator {
            type Item

            function next(self) -> Self.Item? throws never
        }

        interface Sorted requires Iterator {
            function sorted(self) -> Self.Item[] throws never
        }

        class Ints {
            items: int[]

            implements Iterator {
                type Item = int

                function next(self) -> int? {
                    return null
                }
            }

            implements Sorted {
                function sorted(self) -> int[] {
                    return self.items
                }
            }
        }
        "#,
    );
}

#[test]
fn qualified_typevar_projection_disambiguates_required_interfaces() {
    assert_zero_compile_errors(
        r#"
        interface Iterator {
            type Item

            function next(self) -> Self.Item? throws never
        }

        interface Reader {
            type Item

            function read(self) -> Self.Item throws never
        }

        interface Stream requires Iterator, Reader {}

        function head<S extends Stream>(stream: S) -> (S as Iterator).Item? {
            return stream.next()
        }
        "#,
    );
}

#[test]
fn ambiguous_typevar_projection_across_required_interfaces_errors() {
    assert_compile_error_contains(
        r#"
        interface Iterator {
            type Item

            function next(self) -> Self.Item? throws never
        }

        interface Reader {
            type Item

            function read(self) -> Self.Item throws never
        }

        interface Stream requires Iterator, Reader {}

        function bad<S extends Stream>(stream: S) -> S.Item? {
            return stream.next()
        }
        "#,
        "ambiguous associated type projection",
    );
}

#[test]
fn selected_projection_rejects_mismatched_associated_type_binding() {
    assert_compile_error_code(
        r#"
        class TextFormat {}

        interface Codec<Format> {
            type Output

            function decode(self, input: string) -> Self.Output throws never
        }

        class Document {
            raw: string

            implements Codec<TextFormat> {
                type Output=string

                function decode(self, input: string) -> string {
                    return self.raw
                }
            }
        }

        function bad(doc: Document) -> (Document as Codec<TextFormat, Output = int>).Output {
            return doc.as<Codec<TextFormat, Output = string>>.decode("")
        }
        "#,
        "E0001",
    );
}

#[test]
fn qualified_projection_requires_base_to_implement_interface() {
    assert_compile_error_contains(
        r#"
        interface Iterator {
            type Item
        }

        class Box {}

        type Bad = (Box as Iterator).Item
        "#,
        "does not implement interface",
    );
}

#[test]
fn qualified_projection_unknown_associated_member_errors() {
    assert_compile_error_contains(
        r#"
        interface Iterator {
            type Item
        }

        class IntIterator {
            implements Iterator {
                type Item = int
            }
        }

        type Bad = (IntIterator as Iterator).Element
        "#,
        "unknown associated type `Element`",
    );
}

#[test]
fn qualified_projection_requires_interface_qualifier() {
    assert_compile_error_contains(
        r#"
        class IntIterator {}

        type Bad = (IntIterator as IntIterator).Item
        "#,
        "qualified associated type projection must use an interface",
    );
}

#[test]
fn qualified_projection_on_unbounded_typevar_requires_interface_bound() {
    assert_compile_error_contains(
        r#"
        interface HasKey {
            type Key
            key: Self.Key
        }

        function bad<T>(x: (T as HasKey).Key) -> (T as HasKey).Key {
            return x
        }
        "#,
        "type `T` does not implement interface `HasKey`",
    );
}

#[test]
fn qualified_projection_on_typevar_rejects_unproven_interface_bound() {
    assert_compile_error_contains(
        r#"
        interface HasKey {
            type Key
            key: Self.Key
        }

        interface Entity requires HasKey<Key = string> {}

        interface Other {
            type Key
            key: Self.Key
        }

        function bad<T extends Entity>(x: (T as Other).Key) -> (T as Other).Key {
            return x
        }
        "#,
        "type `T` does not implement interface `Other`",
    );
}

#[test]
fn qualified_projection_on_typevar_rejects_conflicting_associated_binding() {
    assert_compile_error_contains(
        r#"
        interface HasKey {
            type Key
            key: Self.Key
        }

        interface Entity requires HasKey<Key = string> {}

        function bad<T extends Entity>(x: (T as HasKey<Key = int>).Key) -> (T as HasKey<Key = int>).Key {
            return x
        }
        "#,
        "type `T` does not implement interface `HasKey<Key = int>`",
    );
}

#[test]
fn qualified_projection_on_typevar_accepts_proven_interface_bound() {
    assert_zero_compile_errors(
        r#"
        interface HasKey {
            type Key
            key: Self.Key
        }

        interface Entity requires HasKey<Key = string> {}

        function ok<T extends Entity>(x: (T as HasKey).Key) -> (T as HasKey).Key {
            return x
        }
        "#,
    );
}

#[test]
fn generic_interface_associated_type_bindings_compile() {
    assert_zero_compile_errors(
        r#"
        interface Cache<K> {
            type Value

            function get(self, key: K) -> Self.Value? throws never
            function put(self, key: K, value: Self.Value) -> null throws never
        }

        class StringIntCache {
            implements Cache<string> {
                type Value = int

                function get(self, key: string) -> int? {
                    return null
                }

                function put(self, key: string, value: int) -> null {
                    return null
                }
            }
        }

        function read<C extends Cache<string>>(cache: C, key: string) -> C.Value? {
            return cache.get(key)
        }

        function main() -> int? {
            return read<StringIntCache>(StringIntCache {}, "answer")
        }
        "#,
    );
}

#[test]
fn generic_associated_type_bindings_infer_through_params_and_lets() {
    assert_zero_compile_errors(
        r#"
        interface Iterator {
            type Item

            function next(self) -> Self.Item? throws never
        }

        function take_one<T>(it: Iterator<Item = T>) -> T? {
            let same: Iterator<Item = T> = it
            return same.next()
        }

        function take_int<I extends Iterator<Item = int>>(it: I) -> int? {
            return it.next()
        }
        "#,
    );
}

#[test]
fn qualified_associated_type_projection_disambiguates_generic_interfaces() {
    assert_zero_compile_errors(
        r#"
        class TextFormat {}
        class CodeFormat {}

        interface Codec<Format> {
            type Output

            function decode(self, input: string) -> Self.Output throws never
        }

        class Document {
            raw: string

            implements Codec<TextFormat> {
                type Output = string

                function decode(self, input: string) -> string {
                    return self.raw
                }
            }

            implements Codec<CodeFormat> {
                type Output = int

                function decode(self, input: string) -> int {
                    return 200
                }
            }
        }

        function decode_text(doc: Document) -> (Document as Codec<TextFormat>).Output {
            return doc.as<Codec<TextFormat, Output = string>>.decode("")
        }

        function decode_code(doc: Document) -> (Document as Codec<CodeFormat>).Output {
            return doc.as<Codec<CodeFormat, Output = int>>.decode("")
        }
        "#,
    );
}

#[test]
fn associated_type_default_can_reference_interface_generic() {
    assert_zero_compile_errors(
        r#"
        interface Boxed<T> {
            type Item = T

            function get(self) -> Self.Item throws never
        }

        class StringBox {
            value: string

            implements Boxed<string> {
                function get(self) -> string {
                    return self.value
                }
            }
        }

        function read_box(box: Boxed<string>) -> string {
            return box.get()
        }
        "#,
    );
}

#[test]
fn associated_type_default_can_reference_explicit_witness() {
    assert_zero_compile_errors(
        r#"
        interface Batch {
            type Item
            type Items = Self.Item[]

            function all(self) -> Self.Items throws never
        }

        class IntBatch {
            values: int[]

            implements Batch {
                type Item = int

                function all(self) -> int[] {
                    return self.values
                }
            }
        }

        function read(batch: Batch<Item = int>) -> int[] {
            return batch.all()
        }
        "#,
    );
}

#[test]
fn associated_type_default_from_qualified_interface_resolves_declaring_namespace() {
    assert_zero_compile_errors_multi(&[
        (
            "ns_contracts/contracts.baml",
            r#"
            class DefaultValue {
                value: int
            }

            interface Cache {
                type Value = DefaultValue

                function get(self) -> Self.Value throws never
            }
            "#,
        ),
        (
            "ns_models/models.baml",
            r#"
            class LocalCache {
                implements root.contracts.Cache {
                    function get(self) -> root.contracts.DefaultValue {
                        return root.contracts.DefaultValue { value: 1 }
                    }
                }
            }
            "#,
        ),
    ]);
}

#[test]
fn associated_type_default_can_reference_declaring_namespace_type_from_implementor_namespace() {
    assert_zero_compile_errors_multi(&[
        (
            "ns_lib/types.baml",
            r#"
            interface Serializable {
                type Format = Payload

                function serialize(self) -> Self.Format throws never
            }

            class Payload {
                data: string
            }
            "#,
        ),
        (
            "ns_app/widget.baml",
            r#"
            class Widget {
                name: string

                implements root.lib.Serializable {
                    function serialize(self) -> Self.Format {
                        return root.lib.Payload { data: self.name }
                    }
                }
            }
            "#,
        ),
    ]);
}

#[test]
fn explicit_associated_type_witness_can_reference_earlier_witness() {
    assert_zero_compile_errors(
        r#"
        interface Batch {
            type Item
            type Items

            function all(self) -> Self.Items throws never
        }

        class IntBatch {
            values: int[]

            implements Batch {
                type Item = int
                type Items = Self.Item[]

                function all(self) -> int[] {
                    return self.values
                }
            }
        }

        function read(batch: Batch<Item = int, Items = int[]>) -> int[] {
            return batch.all()
        }
        "#,
    );
}

#[test]
fn dependent_associated_type_bound_bare_projection_is_not_an_interface() {
    // Only interfaces can be bounds. A bare associated-type projection (`extends
    // Self.Item`) is a non-interface bound, not a dependent bound. The valid way to
    // express a Self-dependent bound is to wrap the projection as an interface's
    // generic argument — see `..._through_interface_resolves_self` below.
    assert_compile_error_contains(
        r#"
        interface Parser {
            type Item
            type Output extends Self.Item

            function parse(self) -> Self.Output throws never
        }
        "#,
        "is not an interface",
    );
}

#[test]
fn dependent_associated_type_bound_through_interface_resolves_self() {
    // A Self-dependent bound goes through an interface: `type Output extends
    // Producer<Self.Item>` requires the implementor's `Output` to implement
    // `Producer<Item>`. `Self.Item` resolves inside the bound's generic argument and
    // realizes at the impl's `Item` binding (`Producer<int>` for `IntParser`).
    assert_zero_compile_errors(
        r#"
        interface Producer<T> {
            function make(self) -> T throws never
        }

        interface Parser {
            type Item
            type Output extends Producer<Self.Item>

            function parse(self) -> Self.Output throws never
        }

        class IntProducer {
            value: int

            implements Producer<int> {
                function make(self) -> int {
                    return self.value
                }
            }
        }

        class IntParser {
            implements Parser {
                type Item = int
                type Output = IntProducer

                function parse(self) -> IntProducer {
                    return IntProducer { value: 1 }
                }
            }
        }

        function parse(parser: Parser<Item = int, Output = IntProducer>) -> IntProducer {
            return parser.parse()
        }
        "#,
    );
}

#[test]
fn dependent_associated_type_bound_through_interface_rejects_non_implementor() {
    // The wrapped Self-dependent bound is enforced: binding `Output` to a type that
    // does NOT implement `Producer<Item>` is rejected at the impl site.
    assert_compile_error_contains(
        r#"
        interface Producer<T> {
            function make(self) -> T throws never
        }

        interface Parser {
            type Item
            type Output extends Producer<Self.Item>
        }

        class IntParser {
            implements Parser {
                type Item = int
                type Output = string
            }
        }
        "#,
        "does not implement bound `Producer<int>`",
    );
}

#[test]
fn associated_type_binding_order_is_not_semantic() {
    assert_zero_compile_errors(
        r#"
        interface Pair {
            type Left
            type Right
        }

        function reorder(pair: Pair<Left = int, Right = string>) -> Pair<Right = string, Left = int> {
            return pair
        }
        "#,
    );
}

#[test]
fn associated_type_binding_overrides_default_on_interface_value() {
    assert_zero_compile_errors(
        r#"
        interface Decoder {
            type Output = string

            function decode(self, input: string) -> Self.Output throws never
        }

        class StatusDecoder {
            implements Decoder {
                type Output = int

                function decode(self, input: string) -> int {
                    return 200
                }
            }
        }

        function decode_status(decoder: Decoder<Output = int>) -> int {
            return decoder.decode("")
        }
        "#,
    );
}

#[test]
fn mixed_generic_args_and_associated_type_bindings_compile() {
    assert_zero_compile_errors(
        r#"
        class TextFormat {}

        interface Codec<Format> {
            type Output = string

            function decode(self, input: string) -> Self.Output throws never
        }

        class Document {
            raw: string

            implements Codec<TextFormat> {
                type Output = string

                function decode(self, input: string) -> string {
                    return self.raw
                }
            }
        }

        function decode_value(decoder: Codec<TextFormat, Output = string>) -> string {
            return decoder.decode("")
        }

        function decode_generic<D extends Codec<TextFormat, Output = string>>(decoder: D) -> string {
            return decoder.decode("")
        }

        function decode_as(doc: Document) -> (Document as Codec<TextFormat, Output = string>).Output {
            return doc.as<Codec<TextFormat, Output = string>>.decode("")
        }
        "#,
    );
}

#[test]
fn named_associated_type_bindings_are_whitespace_insensitive() {
    assert_zero_compile_errors(
        r#"
        class TextFormat {}

        interface Sink {
            type Item
            type Error = string
        }

        interface FormattedSink<Format> {
            type Item
        }

        type Leading = Sink< Item = int>
        type TwoNamed = Sink<Item = int, Error = string>
        type TwoNamedNoSpace = Sink<Item=int,Error=string>
        type PosThen = FormattedSink<TextFormat, Item = int>
        type PosThenNoSpace = FormattedSink<TextFormat,Item=int>
        type Nested = Sink<Item = map<string, int[]>>
        "#,
    );
}

#[test]
fn associated_type_bindings_allow_trailing_commas_in_type_args() {
    assert_zero_compile_errors(
        r#"
        class TextFormat {}

        interface Sink {
            type Item
        }

        interface Codec<Format> {
            type Output

            function decode(self, input: string) -> Self.Output throws never
        }

        class Document {
            raw: string

            implements Codec<TextFormat,> {
                type Output = string

                function decode(self, input: string) -> string {
                    return self.raw
                }
            }
        }

        type One = Sink<Item = int,>
        type Mixed = Codec<TextFormat, Output = string,>

        function decode(doc: Document) -> string {
            return doc.as<Codec<TextFormat, Output = string,>>.decode("")
        }
        "#,
    );
}

#[test]
fn associated_type_binding_can_use_qualified_projection() {
    assert_zero_compile_errors(
        r#"
        class TextFormat {}

        interface Codec<Format> {
            type Output
        }

        interface Sink {
            type Item
        }

        class Document {
            implements Codec<TextFormat> {
                type Output = string
            }
        }

        type ProjectedSink = Sink<Item = (Document as Codec<TextFormat>).Output>
        "#,
    );
}

#[test]
fn complex_qualified_projection_type_args_compile() {
    assert_zero_compile_errors(
        r#"
        class TextFormat {}
        class CodeFormat {}
        class PairFormat {}

        interface Codec<Format> {
            type Output
        }

        class Document {
            implements Codec<TextFormat> {
                type Output = string
            }

            implements Codec<map<string, PairFormat[]?>> {
                type Output = string
            }

            implements Codec<(TextFormat | CodeFormat)> {
                type Output = int
            }

            implements Codec<(value: string) -> int> {
                type Output = bool
            }
        }

        type TextOut = (Document as Codec<TextFormat>).Output
        type MaybeTextOut = (Document as Codec<TextFormat>).Output?
        type TextOutList = (Document as Codec<TextFormat>).Output[]
        type TextOutMap = map<string, (Document as Codec<TextFormat>).Output>
        type MapArgOut = (Document as Codec<map<string, PairFormat[]?>>).Output
        type UnionArgOut = (Document as Codec<(TextFormat | CodeFormat)>).Output
        type FunctionArgOut = (Document as Codec<(value: string) -> int>).Output
        type WrappedBase = ((Document) as Codec<TextFormat>).Output
        type WrappedInterface = (Document as (Codec<TextFormat>)).Output
        "#,
    );
}

#[test]
fn associated_type_bindings_can_be_union_types() {
    assert_zero_compile_errors(
        r#"
        interface Source {
            type Item

            function get(self) -> Self.Item throws never
            function maybe(self) -> Self.Item? throws never
            function either(self) -> Self.Item | bool throws never
        }

        class MixedSource {
            implements Source {
                type Item = int | string

                function get(self) -> int | string {
                    return 1
                }

                function maybe(self) -> (int | string)? {
                    return null
                }

                function either(self) -> int | string | bool {
                    return true
                }
            }
        }

        function read(source: Source<Item = int | string>) -> int | string {
            return source.get()
        }

        function read_maybe(source: Source<Item = int | string>) -> (int | string)? {
            return source.maybe()
        }

        function read_either(source: Source<Item = int | string>) -> int | string | bool {
            return source.either()
        }
        "#,
    );
}

#[test]
fn associated_type_union_defaults_can_reference_interface_generics() {
    assert_zero_compile_errors(
        r#"
        interface Response<T> {
            type Payload = T | null

            function payload(self) -> Self.Payload throws never
        }

        class StringResponse {
            implements Response<string> {
                function payload(self) -> string | null {
                    return null
                }
            }
        }

        function read(response: Response<string>) -> string | null {
            return response.payload()
        }
        "#,
    );
}

#[test]
fn associated_type_bounds_reject_union_types() {
    // Bounds are interfaces only — an associated type's `extends` bound may not be a
    // union (there is no `implements` relation to a union type).
    assert_compile_error_contains(
        r#"
        interface Parser {
            type Output extends int | string

            function parse(self) -> Self.Output throws never
        }
        "#,
        "is not an interface",
    );
}

#[test]
fn union_associated_type_bindings_work_in_generic_bounds() {
    assert_zero_compile_errors(
        r#"
        interface Source {
            type Item

            function get(self) -> Self.Item throws never
        }

        class MixedSource {
            implements Source {
                type Item = int | string

                function get(self) -> int | string {
                    return "value"
                }
            }
        }

        function consume<S extends Source<Item = int | string>>(source: S) -> int | string {
            return source.get()
        }

        function main(source: MixedSource) -> int | string {
            return consume<MixedSource>(source)
        }
        "#,
    );
}

#[test]
fn associated_type_binding_in_generic_bound_preserves_outer_typevar() {
    assert_zero_compile_errors(
        r#"
        interface Source {
            type Item

            function get(self) -> Self.Item throws never
        }

        class IntSource {
            value: int

            implements Source {
                type Item = int

                function get(self) -> int {
                    return self.value
                }
            }
        }

        function score_bound<T, S extends Source<Item = T>>(source: S) -> T {
            return source.get()
        }

        function main() -> int {
            let source = IntSource { value: 42 }
            return score_bound<int, IntSource>(source)
        }
        "#,
    );
}

#[test]
fn union_associated_type_binding_in_generic_bound_preserves_outer_typevar() {
    assert_zero_compile_errors(
        r#"
        interface Source {
            type Item

            function get(self) -> Self.Item throws never
        }

        class MixedSource {
            implements Source {
                type Item = int | string

                function get(self) -> int | string {
                    return 1
                }
            }
        }

        function score_bound<T, S extends Source<Item = T>>(source: S) -> T {
            return source.get()
        }

        function main(source: MixedSource) -> int | string {
            return score_bound<int | string, MixedSource>(source)
        }
        "#,
    );
}

#[test]
fn nested_associated_type_bindings_in_generic_bounds_preserve_outer_typevar() {
    assert_zero_compile_errors(
        r#"
        interface Source {
            type Item

            function get(self) -> Self.Item throws never
        }

        class OptionalIntSource {
            value: int?

            implements Source {
                type Item = int?

                function get(self) -> int? {
                    return self.value
                }
            }
        }

        class IntListSource {
            value: int[]

            implements Source {
                type Item = int[]

                function get(self) -> int[] {
                    return self.value
                }
            }
        }

        class IntMapSource {
            value: map<string, int>

            implements Source {
                type Item = map<string, int>

                function get(self) -> map<string, int> {
                    return self.value
                }
            }
        }

        class IntCallbackSource {
            value: (x: int) -> int

            implements Source {
                type Item = (x: int) -> int

                function get(self) -> (x: int) -> int {
                    return self.value
                }
            }
        }

        function read_optional<T, S extends Source<Item = T?>>(source: S) -> T? {
            return source.get()
        }

        function read_list<T, S extends Source<Item = T[]>>(source: S) -> T[] {
            return source.get()
        }

        function read_map<T, S extends Source<Item = map<string, T>>>(source: S) -> map<string, T> {
            return source.get()
        }

        function read_callback<T, S extends Source<Item = (x: T) -> T>>(source: S) -> (x: T) -> T {
            return source.get()
        }

        function id(value: int) -> int {
            return value
        }

        function use_optional() -> int? {
            return read_optional<int, OptionalIntSource>(OptionalIntSource { value: 1 })
        }

        function use_list() -> int[] {
            return read_list<int, IntListSource>(IntListSource { value: [2] })
        }

        function use_map() -> map<string, int> {
            return read_map<int, IntMapSource>(IntMapSource { value: { "x": 3 } })
        }

        function use_callback() -> (x: int) -> int {
            return read_callback<int, IntCallbackSource>(IntCallbackSource { value: id })
        }
        "#,
    );
}

#[test]
fn qualified_projections_disambiguate_union_outputs() {
    assert_zero_compile_errors(
        r#"
        class TextFormat {}
        class CodeFormat {}

        interface Codec<Format> {
            type Output

            function decode(self, input: string) -> Self.Output throws never
        }

        class Document {
            raw: string

            implements Codec<TextFormat> {
                type Output = string | null

                function decode(self, input: string) -> string | null {
                    return self.raw
                }
            }

            implements Codec<CodeFormat> {
                type Output = int | bool

                function decode(self, input: string) -> int | bool {
                    return 200
                }
            }
        }

        type AnyDecoded =
            (Document as Codec<TextFormat>).Output | (Document as Codec<CodeFormat>).Output

        function decode_text(doc: Document) -> (Document as Codec<TextFormat>).Output {
            return doc.as<Codec<TextFormat, Output = string | null>>.decode("")
        }

        function decode_any(doc: Document) ->
            (Document as Codec<TextFormat>).Output | (Document as Codec<CodeFormat>).Output {
            return doc.as<Codec<CodeFormat, Output = int | bool>>.decode("")
        }
        "#,
    );
}

#[test]
fn aliased_qualified_projection_unions_resolve_like_inline_unions() {
    assert_zero_compile_errors(
        r#"
        class TextFormat {}
        class CodeFormat {}

        interface Codec<Format> {
            type Output

            function decode(self, input: string) -> Self.Output throws never
        }

        class Document {
            raw: string

            implements Codec<TextFormat> {
                type Output = string | int

                function decode(self, input: string) -> string | int {
                    return self.raw
                }
            }

            implements Codec<CodeFormat> {
                type Output = int | bool

                function decode(self, input: string) -> int | bool {
                    return 200
                }
            }
        }

        type DecodeOutput =
            (Document as Codec<TextFormat>).Output | (Document as Codec<CodeFormat>).Output

        function decode_union(doc: Document, text: bool) -> DecodeOutput {
            if text {
                return doc.as<Codec<TextFormat, Output = string | int>>.decode("")
            }
            return doc.as<Codec<CodeFormat, Output = int | bool>>.decode("")
        }
        "#,
    );
}

#[test]
fn selected_union_associated_output_rejects_incompatible_return() {
    assert_compile_error_code(
        r#"
        class TextFormat {}

        interface Codec<Format> {
            type Output

            function decode(self, input: string) -> Self.Output throws never
        }

        class Document {
            raw: string

            implements Codec<TextFormat> {
                type Output = string | int

                function decode(self, input: string) -> string | int {
                    return self.raw
                }
            }
        }

        function bad(doc: Document) -> bool {
            return doc.as<Codec<TextFormat, Output = string | int>>.decode("")
        }
        "#,
        "E0001",
    );
}

#[test]
fn match_narrowing_distinguishes_interface_associated_bindings_in_union() {
    assert_zero_compile_errors(
        r#"
        interface Iterator {
            type Item

            function next(self) -> Self.Item throws never
        }

        class IntIterator {
            implements Iterator {
                type Item = int

                function next(self) -> int {
                    return 1
                }
            }
        }

        class StringIterator {
            implements Iterator {
                type Item = string

                function next(self) -> string {
                    return "s"
                }
            }
        }

        function label(it: Iterator<Item = int> | Iterator<Item = string>) -> string {
            return match (it) {
                let ints: Iterator<Item = int> => "int",
                _ => "other",
            }
        }
        "#,
    );
}

#[test]
fn match_narrowing_partitions_interface_associated_bindings_in_union() {
    assert_zero_compile_errors(
        r#"
        interface Iterator {
            type Item

            function next(self) -> Self.Item throws never
        }

        class IntIterator {
            implements Iterator {
                type Item = int

                function next(self) -> int {
                    return 1
                }
            }
        }

        class StringIterator {
            implements Iterator {
                type Item = string

                function next(self) -> string {
                    return "s"
                }
            }
        }

        function label(it: Iterator<Item = int> | Iterator<Item = string>) -> string {
            return match (it) {
                let ints: Iterator<Item = int> => "int",
                let strings: Iterator<Item = string> => "string",
            }
        }
        "#,
    );
}

#[test]
fn narrowed_associated_interface_pattern_does_not_exhaust_unbound_interface() {
    assert_compile_error_code(
        r#"
        interface Iterator {
            type Item

            function next(self) -> Self.Item throws never
        }

        function label(it: Iterator) -> string {
            return match (it) {
                let ints: Iterator<Item = int> => "int",
            }
        }
        "#,
        "E0062",
    );
}

#[test]
fn interface_destructure_substitutes_associated_field_type() {
    assert_zero_compile_errors(
        r#"
        interface Source {
            type Item

            value: Self.Item
        }

        class IntSource {
            value: int

            implements Source {
                type Item = int
            }
        }

        function read(source: Source<Item = int>) -> int {
            return match (source) {
                Source { value } => value
            }
        }
        "#,
    );
}

#[test]
fn associated_type_union_substitutes_inside_nested_containers() {
    assert_zero_compile_errors(
        r#"
        interface Source {
            type Item

            function list(self) -> Self.Item[] throws never
            function table(self) -> map<string, Self.Item | null> throws never
        }

        class MixedSource {
            values: (int | string)[]
            table_values: map<string, int | string | null>

            implements Source {
                type Item = int | string

                function list(self) -> (int | string)[] {
                    return self.values
                }

                function table(self) -> map<string, int | string | null> {
                    return self.table_values
                }
            }
        }

        function list(source: Source<Item = int | string>) -> (int | string)[] {
            return source.list()
        }

        function table(source: Source<Item = int | string>) -> map<string, int | string | null> {
            return source.table()
        }
        "#,
    );
}

#[test]
fn formatter_accepts_associated_type_syntax() {
    let source = r#"
        class TextFormat {}

        interface Codec<Format> {
            type Output

            function decode(self, input: string) -> Self.Output throws never
        }

        class Document {
            raw: string

            implements Codec<TextFormat> {
                type Output = string

                function decode(self, input: string) -> string {
                    return self.raw
                }
            }
        }

        type TextOut = (Document as Codec<TextFormat, Output = string>).Output

        interface Source {
            type Item

            function get(self) -> Self.Item throws never
        }

        type TrailingSource = Source<Item = int,>
        type MultilineSource = Source<
            // item witness
            Item = int,
        >

        function takes_bound<S extends Source<Item = int | string>>(source: S) -> S.Item {
            return source.get()
        }

        function decode_as(doc: Document) -> (Document as Codec<TextFormat, Output = string>).Output {
            let output: (Document as Codec<TextFormat>).Output = doc.as<Codec<TextFormat, Output = string>>.decode("")
            return output
        }
        "#;

    let formatted = baml_fmt::format(source, &FormatOptions::default())
        .expect("formatter should accept associated type syntax");
    assert!(formatted.contains("type Output = string"));
    assert!(formatted.contains("Codec<TextFormat, Output = string>"));
    assert!(formatted.contains("(Document as Codec<TextFormat>).Output"));
    assert!(formatted.contains("S extends Source<Item = int | string>"));
    assert!(formatted.contains("TrailingSource"));
    assert!(formatted.contains("// item witness"));
}

#[test]
fn out_of_body_implements_can_bind_associated_types() {
    assert_zero_compile_errors(
        r#"
        interface Showable {
            type Repr

            function repr(self) -> Self.Repr throws never
        }

        class Meter {
            value: int
        }

        implements Showable for Meter {
            type Repr = string

            function repr(self) -> string {
                return "meter"
            }
        }

        function render(meter: Meter) -> Meter.Repr {
            return meter.repr()
        }
        "#,
    );
}

#[test]
fn out_of_body_implements_target_associated_type_binding_errors() {
    assert_compile_error_contains(
        r#"
        interface Iterator {
            type Item

            function next(self) -> Self.Item? throws never
        }

        class IntIterator {}

        implements Iterator<Item = int> for IntIterator {
            type Item = int

            function next(self) -> int? {
                return null
            }
        }
        "#,
        "associated type bindings are not allowed on an `implements` target",
    );
}

#[test]
fn concrete_class_associated_projection_accepts_bound_type() {
    assert_zero_compile_errors(
        r#"
        interface Carrier {
            type Item

            function get(self) -> Self.Item throws never
        }

        class IntCarrier {
            implements Carrier {
                type Item = int

                function get(self) -> int {
                    return 1
                }
            }
        }

        function use_item(carrier: IntCarrier) -> int {
            let item: IntCarrier.Item = carrier.get()
            return item
        }
        "#,
    );
}

#[test]
fn associated_type_projection_expands_class_type_alias_base() {
    assert_zero_compile_errors(
        r#"
        interface Iterator {
            type Item

            function next(self) -> Self.Item? throws never
        }

        class IntIterator {
            implements Iterator {
                type Item = int

                function next(self) -> int? {
                    return null
                }
            }
        }

        type AliasIterator = IntIterator

        function next(it: IntIterator) -> AliasIterator.Item? {
            return it.next()
        }
        "#,
    );
}

#[test]
fn associated_type_projection_expands_fully_bound_interface_alias_base() {
    assert_zero_compile_errors(
        r#"
        interface Iterator {
            type Item

            function next(self) -> Self.Item? throws never
        }

        class IntIterator {
            implements Iterator {
                type Item = int

                function next(self) -> int? {
                    return null
                }
            }
        }

        type IntIteratorInterface = Iterator<Item = int>

        function next(it: IntIteratorInterface) -> IntIteratorInterface.Item? {
            return it.next()
        }
        "#,
    );
}

#[test]
fn associated_types_substitute_inside_nested_type_positions() {
    assert_zero_compile_errors(
        r#"
        interface Mapper {
            type Item

            function list(self) -> Self.Item[] throws never
            function table(self) -> map<string, Self.Item?> throws never
            function choose(self) -> Self.Item? throws never
        }

        class IntMapper {
            items: int[]
            values: map<string, int?>

            implements Mapper {
                type Item = int

                function list(self) -> int[] {
                    return self.items
                }

                function table(self) -> map<string, int?> {
                    return self.values
                }

                function choose(self) -> int? {
                    return null
                }
            }
        }

        function table(mapper: Mapper<Item = int>) -> map<string, int?> {
            return mapper.table()
        }
        "#,
    );
}

#[test]
fn associated_types_substitute_inside_function_type_positions() {
    assert_zero_compile_errors(
        r#"
        interface Lifter {
            type Item

            function lift(self) -> ((Self.Item) -> Self.Item?) throws never
        }

        function lift_int(lifter: Lifter<Item = int>) -> (int) -> int? {
            return lifter.lift()
        }
        "#,
    );
}

#[test]
fn qualified_projection_works_in_nested_types() {
    assert_zero_compile_errors(
        r#"
        class TextFormat {}

        interface Codec<Format> {
            type Output

            function decode(self, input: string) -> Self.Output throws never
        }

        class Document {
            raw: string

            implements Codec<TextFormat> {
                type Output = string

                function decode(self, input: string) -> string {
                    return self.raw
                }
            }
        }

        function decode_many(doc: Document) -> (Document as Codec<TextFormat>).Output[] {
            return [doc.as<Codec<TextFormat, Output = string>>.decode("")]
        }
        "#,
    );
}

#[test]
fn qualified_projection_works_in_local_type_annotations() {
    assert_zero_compile_errors(
        r#"
        class TextFormat {}

        interface Codec<Format> {
            type Output

            function decode(self, input: string) -> Self.Output throws never
        }

        class Document {
            raw: string

            implements Codec<TextFormat> {
                type Output = string

                function decode(self, input: string) -> string {
                    return self.raw
                }
            }
        }

        function decode(doc: Document) -> string {
            let output: (Document as Codec<TextFormat>).Output = doc.as<Codec<TextFormat, Output = string>>.decode("")
            return output
        }
        "#,
    );
}

#[test]
fn unbound_interface_existential_requires_pins_even_for_non_associated_methods() {
    // Strict existential-pin rule (§1.7): an interface used as a value type must pin
    // every non-defaulted associated type, even when the code only calls methods that
    // do not mention it (`size()` here). `SizedIterator` (unpinned) is rejected.
    assert_compile_error_contains(
        r#"
        interface SizedIterator {
            type Item

            function size(self) -> int throws never
            function next(self) -> Self.Item? throws never
        }

        class IntIterator {
            implements SizedIterator {
                type Item = int

                function size(self) -> int {
                    return 1
                }

                function next(self) -> int? {
                    return null
                }
            }
        }

        function count(it: SizedIterator) -> int {
            return it.size()
        }
        "#,
        "must specify its associated type",
    );
}

#[test]
fn unbound_interface_existential_projection_requires_pins() {
    // A bare-existential parameter (`SizedIterator`, `Item` unpinned) is rejected
    // regardless of how its associated projection is used downstream.
    assert_compile_error_contains(
        r#"
        interface SizedIterator {
            type Item

            function next(self) -> Self.Item? throws never
        }

        function next(it: SizedIterator) -> SizedIterator.Item? {
            return it.next()
        }
        "#,
        "must specify its associated type",
    );
}

#[test]
fn unbound_interface_method_that_returns_associated_type_errors() {
    assert_compile_error_code(
        r#"
        interface Iterator {
            type Item

            function next(self) -> Self.Item? throws never
        }

        class IntIterator {
            implements Iterator {
                type Item = int

                function next(self) -> int? {
                    return null
                }
            }
        }

        function bad(it: Iterator) -> int? {
            return it.next()
        }
        "#,
        "E0001",
    );
}

#[test]
fn concrete_class_associated_projection_rejects_wrong_type() {
    assert_compile_error_code(
        r#"
        interface Carrier {
            type Item

            function get(self) -> Self.Item throws never
        }

        class IntCarrier {
            implements Carrier {
                type Item = int

                function get(self) -> int {
                    return 1
                }
            }
        }

        function bad(carrier: IntCarrier) -> string {
            return carrier.get()
        }
        "#,
        "E0001",
    );
}

#[test]
fn associated_function_type_projection_rejects_wrong_nested_type() {
    assert_compile_error_code(
        r#"
        interface Lifter {
            type Item

            function lift(self) -> ((Self.Item) -> Self.Item?) throws never
        }

        function bad(lifter: Lifter<Item = int>) -> (string) -> string? {
            return lifter.lift()
        }
        "#,
        "E0001",
    );
}

#[test]
fn missing_required_associated_type_binding_errors() {
    assert_compile_error_contains(
        r#"
        interface Iterator {
            type Item

            function next(self) -> Self.Item? throws never
        }

        class BadIterator {
            implements Iterator {
                function next(self) -> int? {
                    return null
                }
            }
        }
        "#,
        "missing associated type binding",
    );
}

#[test]
fn duplicate_associated_type_declaration_errors() {
    assert_compile_error_code(
        r#"
        interface Bad {
            type Item
            type Item = int
        }
        "#,
        "E0012",
    );
}

#[test]
fn associated_type_cannot_collide_with_interface_generic_param() {
    assert_compile_error_contains(
        r#"
        interface Container<Item> {
            type Item
        }
        "#,
        "collides with generic parameter",
    );
}

#[test]
fn duplicate_associated_type_binding_errors() {
    assert_compile_error_contains(
        r#"
        interface Iterator {
            type Item
        }

        class BadIterator {
            implements Iterator {
                type Item = int
                type Item = string
            }
        }
        "#,
        "is bound more than once",
    );
}

#[test]
fn implements_target_associated_type_binding_errors() {
    assert_compile_error_contains(
        r#"
        interface Iterator {
            type Item

            function next(self) -> Self.Item? throws never
        }

        class IntIterator {
            implements Iterator<Item = int> {
                function next(self) -> int? {
                    return null
                }
            }
        }
        "#,
        "associated type bindings are not allowed on an `implements` target",
    );
}

#[test]
fn implements_target_associated_type_binding_errors_even_when_same_witness_is_in_body() {
    assert_compile_error_contains(
        r#"
        interface Iterator {
            type Item

            function next(self) -> Self.Item? throws never
        }

        class IntIterator {
            implements Iterator<Item = int> {
                type Item = int

                function next(self) -> int? {
                    return null
                }
            }
        }
        "#,
        "associated type bindings are not allowed on an `implements` target",
    );
}

#[test]
fn impl_associated_type_witness_rejects_extends_bound() {
    assert_compile_error_contains(
        r#"
        interface Iterator {
            type Item

            function next(self) -> Self.Item? throws never
        }

        class IntIterator {
            implements Iterator {
                type Item extends int = int

                function next(self) -> int? {
                    return null
                }
            }
        }
        "#,
        "associated type bounds are only allowed on interface declarations",
    );
}

#[test]
fn implements_block_associated_type_binding_is_honored() {
    assert_zero_compile_errors(
        r#"
        interface Iterator {
            type Item

            function next(self) -> Self.Item? throws never
        }

        class IntIterator {
            implements Iterator {
                type Item = int

                function next(self) -> int? {
                    return null
                }
            }
        }

        function next(it: IntIterator) -> int? {
            return it.next()
        }
        "#,
    );
}

#[test]
fn implements_target_associated_type_binding_errors_even_with_block_binding() {
    assert_compile_error_contains(
        r#"
        interface Iterator {
            type Item

            function next(self) -> Self.Item? throws never
        }

        class BadIterator {
            implements Iterator<Item = string> {
                type Item = int

                function next(self) -> int? {
                    return null
                }
            }
        }
        "#,
        "associated type bindings are not allowed on an `implements` target",
    );
}

#[test]
fn duplicate_associated_type_binding_on_interface_value_errors() {
    assert_compile_error_contains(
        r#"
        interface Iterator {
            type Item

            function next(self) -> Self.Item? throws never
        }

        function bad(it: Iterator<Item = int, Item = string>) -> int {
            return 0
        }
        "#,
        "is bound more than once",
    );
}

#[test]
fn duplicate_associated_type_binding_on_interface_value_with_union_errors() {
    assert_compile_error_contains(
        r#"
        interface Iterator {
            type Item

            function next(self) -> Self.Item throws never
        }

        function bad(it: Iterator<Item = int | string, Item = string>) -> int {
            return 0
        }
        "#,
        "is bound more than once",
    );
}

#[test]
fn unknown_associated_type_binding_in_implements_errors() {
    assert_compile_error_contains(
        r#"
        interface Iterator {
            type Item
        }

        class BadIterator {
            implements Iterator {
                type Element = int
                type Item = int
            }
        }
        "#,
        "unknown associated type `Element`",
    );
}

#[test]
fn associated_type_bound_failure_errors() {
    assert_compile_error_code(
        r#"
        interface Named {
            name: string
        }

        interface Parser {
            type Output extends Named

            function parse(self) -> Self.Output throws never
        }

        class BadParser {
            implements Parser {
                type Output = int

                function parse(self) -> int {
                    return 1
                }
            }
        }
        "#,
        "E0001",
    );
}

#[test]
fn associated_type_default_bound_failure_errors() {
    assert_compile_error_code(
        r#"
        interface Named {
            name: string
        }

        interface Parser {
            type Output extends Named = int
        }
        "#,
        "E0001",
    );
}

#[tokio::test]
async fn associated_type_default_typevar_satisfies_declared_bound() {
    let output = baml_test!(
        r#"
        interface Summarizable {
            function summary(self) -> string throws never
        }

        interface Holder<T extends Summarizable> {
            type Item extends Summarizable = T

            function get(self) -> Self.Item throws never
        }

        class User {
            name: string

            implements Summarizable {
                function summary(self) -> string {
                    return self.name
                }
            }
        }

        class UserHolder {
            user: User

            implements Holder<User> {
                function get(self) -> Self.Item {
                    return self.user
                }
            }
        }

        function summarize<H extends Holder<User>>(holder: H) -> string {
            return holder.get().summary()
        }

        function main() -> string {
            return summarize(UserHolder { user: User { name: "Ada" } })
        }
        "#
    );

    assert_eq!(
        output.result.unwrap(),
        BexExternalValue::String("Ada".into())
    );
}

#[test]
fn associated_type_binding_def_typevar_satisfies_declared_bound() {
    assert_zero_compile_errors(
        r#"
        interface Summarizable {
            function summary(self) -> string throws never
        }

        interface Holder {
            type Item extends Summarizable
            function get(self) -> Self.Item throws never
        }

        class Box<T> {
            value: T
        }

        implements<T extends Summarizable> Holder for Box<T> {
            type Item = T

            function get(self) -> Self.Item {
                return self.value
            }
        }
        "#,
    );
}

#[test]
fn associated_type_interface_binding_typevar_satisfies_declared_bound() {
    assert_zero_compile_errors(
        r#"
        interface Summarizable {
            function summary(self) -> string throws never
        }

        interface Holder {
            type Item extends Summarizable
            function get(self) -> Self.Item throws never
        }

        function summarize<T extends Summarizable>(holder: Holder<Item = T>) -> string {
            return holder.get().summary()
        }
        "#,
    );
}

#[test]
fn abstract_associated_projection_uses_declared_bound_for_members() {
    assert_zero_compile_errors(
        r#"
        interface Summarizable {
            function summary(self) -> string throws never
        }

        interface Holder {
            type Item extends Summarizable

            function get(self) -> Self.Item throws never
        }

        function summarize<H extends Holder>(holder: H) -> string {
            return holder.get().summary()
        }
        "#,
    );
}

#[test]
fn abstract_associated_projection_pins_self_for_bound_methods() {
    assert_zero_compile_errors(
        r#"
        interface Comparable {
            function same(self, other: Self) -> bool throws never
        }

        interface Holder {
            type Item extends Comparable

            function left(self) -> Self.Item throws never
            function right(self) -> Self.Item throws never
        }

        function compare<H extends Holder>(holder: H) -> bool {
            return holder.left().same(holder.right())
        }
        "#,
    );
}

#[test]
fn abstract_associated_projection_self_param_rejects_unrelated_bound_typevar() {
    assert_compile_error_code(
        r#"
        interface Comparable {
            function same(self, other: Self) -> bool throws never
        }

        interface Holder {
            type Item extends Comparable

            function left(self) -> Self.Item throws never
        }

        function compare<H extends Holder, C extends Comparable>(holder: H, other: C) -> bool {
            return holder.left().same(other)
        }
        "#,
        "E0001",
    );
}

#[tokio::test]
async fn abstract_associated_projection_bound_method_dispatch_runs() {
    let output = baml_test!(
        r#"
        interface SomeInterface {
            function label(self) -> string throws never

            function same_label(self, other: Self) -> bool throws never {
                return self.label() == other.label()
            }
        }

        interface Holder {
            type Item extends SomeInterface

            function get(self) -> Self.Item throws never
        }

        class Widget {
            name: string

            implements SomeInterface {
                function label(self) -> string {
                    return self.name
                }
            }
        }

        class WidgetHolder {
            item: Widget

            implements Holder {
                type Item = Widget

                function get(self) -> Widget {
                    return self.item
                }
            }
        }

        function label_from_holder<H extends Holder>(holder: H) -> string {
            let item = holder.get()
            return item.label()
        }

        function holder_matches_itself<H extends Holder>(holder: H) -> bool {
            let item = holder.get()
            return item.same_label(item)
        }

        function main() -> string {
            let holder = WidgetHolder { item: Widget { name: "alpha" } }
            let same = if holder_matches_itself(holder) { "true" } else { "false" }
            return label_from_holder(holder) + ":" + same
        }
        "#
    );

    assert_eq!(
        output.result.unwrap(),
        BexExternalValue::String("alpha:true".into())
    );
}

#[tokio::test]
async fn inferred_native_generic_type_arg_from_interface_associated_return_runs() {
    let output = baml_test!(
        r#"
        interface Iterator {
            type Item

            function next(self) -> Self.Item throws never
        }

        class IntIterator {
            value: int

            implements Iterator {
                type Item = int?

                function next(self) -> Self.Item {
                    return self.value
                }
            }
        }

        function stringify_next(iter: Iterator<Item = int?>) -> string {
            return baml.json.to_string(iter.next())
        }

        function main() -> string {
            return stringify_next(IntIterator { value: 7 })
        }
        "#
    );

    assert_eq!(output.result.unwrap(), BexExternalValue::String("7".into()));
}

#[test]
fn associated_union_projection_rejects_narrow_interface_return() {
    assert_compile_error_code(
        r#"
        interface Iterator {
            type Item

            function next(self) -> Self.Item throws never
        }

        function bad(it: Iterator<Item = int | string>) -> int {
            return it.next()
        }
        "#,
        "E0001",
    );
}

#[test]
fn associated_union_projection_rejects_narrow_class_return() {
    assert_compile_error_code(
        r#"
        interface Iterator {
            type Item

            function next(self) -> Self.Item throws never
        }

        class MixedIterator {
            value: int | string

            implements Iterator {
                type Item = int | string

                function next(self) -> Self.Item {
                    return self.value
                }
            }
        }

        function bad(it: MixedIterator) -> int {
            return it.next()
        }
        "#,
        "E0001",
    );
}

// Return types are covariant in BAML (like throws): the interface declares
// `Self.Item | string`, which realizes to `int | string` at `IntProducer`, and
// the override's narrower `int` return conforms (`int <: int | string`).
// Conformance is whole-function subtyping — params contravariant, return and
// throws covariant — not the old checker's exact match.
#[test]
fn associated_union_required_method_impl_may_narrow_return_covariantly() {
    assert_zero_compile_errors(
        r#"
        interface Producer {
            type Item

            function produce(self) -> Self.Item | string throws never
        }

        class IntProducer {
            implements Producer {
                type Item = int

                function produce(self) -> int {
                    return 1
                }
            }
        }
        "#,
    );
}

#[test]
fn associated_union_binding_must_satisfy_extends_bound_in_generic_bound() {
    assert_compile_error_contains(
        r#"
        interface Named {
            name: string
        }

        class Label {
            name: string
            implements Named {}
        }

        interface Parser {
            type Output extends Named

            function parse(self) -> Self.Output throws never
        }

        function bad<P extends Parser<Output = Label | int>>(parser: P) -> P.Output {
            return parser.parse()
        }
        "#,
        "does not implement bound",
    );
}

#[test]
fn unknown_associated_type_binding_on_interface_value_errors() {
    assert_compile_error_contains(
        r#"
        interface Iterator {
            type Item

            function next(self) -> Self.Item? throws never
        }

        function bad(it: Iterator<Element = int>) -> int {
            return 0
        }
        "#,
        "Element. Did you mean `Item`",
    );
}

#[test]
fn unknown_associated_type_binding_on_interface_value_with_union_errors() {
    assert_compile_error_contains(
        r#"
        interface Iterator {
            type Item

            function next(self) -> Self.Item throws never
        }

        function bad(it: Iterator<Element = int | string, Item = int>) -> int {
            return 0
        }
        "#,
        "Element. Did you mean `Item`",
    );
}

#[test]
fn ambiguous_unqualified_associated_type_projection_errors() {
    assert_compile_error_code(
        r#"
        interface Iterator {
            type Item

            function next(self) -> Self.Item? throws never
        }

        interface Reader {
            type Item

            function read(self) -> Self.Item throws never
        }

        class File {
            implements Iterator {
                type Item = string

                function next(self) -> string? {
                    return null
                }
            }

            implements Reader {
                type Item = int

                function read(self) -> int {
                    return 1
                }
            }
        }

        function bad(file: File) -> File.Item? {
            return file.next()
        }
        "#,
        "E0001",
    );
}

#[test]
fn ambiguous_unqualified_projection_across_generic_instantiations_errors() {
    assert_compile_error_code(
        r#"
        class TextFormat {}
        class CodeFormat {}

        interface Codec<Format> {
            type Output

            function decode(self, input: string) -> Self.Output throws never
        }

        class Document {
            raw: string

            implements Codec<TextFormat> {
                type Output = string

                function decode(self, input: string) -> string {
                    return self.raw
                }
            }

            implements Codec<CodeFormat> {
                type Output = int

                function decode(self, input: string) -> int {
                    return 200
                }
            }
        }

        function bad(doc: Document) -> Document.Output {
            return doc.as<Codec<TextFormat, Output = string>>.decode("")
        }
        "#,
        "E0001",
    );
}

#[test]
fn ambiguous_unqualified_projection_type_alias_errors() {
    assert_compile_error_contains(
        r#"
        class TextFormat {}
        class CodeFormat {}

        interface Codec<Format> {
            type Output
        }

        class Document {
            implements Codec<TextFormat> {
                type Output = string
            }

            implements Codec<CodeFormat> {
                type Output = int
            }
        }

        type Ambiguous = Document.Output
        "#,
        "ambiguous associated type `Output`",
    );
}

#[test]
fn unknown_unqualified_projection_type_alias_errors() {
    assert_compile_error_contains(
        r#"
        class Document {}

        type Missing = Document.Output
        "#,
        "unknown associated type `Output`",
    );
}

#[test]
fn bare_default_associated_interface_enforces_default_on_assignment() {
    assert_compile_error_code(
        r#"
        interface Decoder {
            type Output = string

            function decode(self, input: string) -> Self.Output throws never
        }

        class StatusDecoder {
            implements Decoder {
                type Output = int

                function decode(self, input: string) -> int {
                    return 200
                }
            }
        }

        function bad() -> string {
            let decoder: Decoder = StatusDecoder {}
            return decoder.decode("")
        }
        "#,
        "E0001",
    );
}

#[test]
fn default_associated_interface_omission_is_positive_for_default_witness() {
    assert_zero_compile_errors(
        r#"
        interface Decoder {
            type Output = string

            function decode(self, input: string) -> Self.Output throws never
        }

        class TextDecoder {
            implements Decoder {
                function decode(self, input: string) -> string {
                    return input
                }
            }
        }

        function decode(decoder: Decoder) -> string {
            return decoder.decode("")
        }

        function main() -> string {
            return decode(TextDecoder {})
        }
        "#,
    );
}

#[test]
fn bare_default_associated_generic_bound_enforces_default() {
    assert_compile_error_code(
        r#"
        interface Decoder {
            type Output = string

            function decode(self, input: string) -> Self.Output throws never
        }

        class StatusDecoder {
            implements Decoder {
                type Output = int

                function decode(self, input: string) -> int {
                    return 200
                }
            }
        }

        function take_default<D extends Decoder>(decoder: D) -> string {
            return decoder.decode("")
        }

        function bad(decoder: StatusDecoder) -> string {
            return take_default<StatusDecoder>(decoder)
        }
        "#,
        "E0001",
    );
}

#[test]
fn bare_default_associated_union_member_enforces_default() {
    assert_compile_error_code(
        r#"
        interface Decoder {
            type Output = string

            function decode(self, input: string) -> Self.Output throws never
        }

        class StatusDecoder {
            implements Decoder {
                type Output = int

                function decode(self, input: string) -> int {
                    return 200
                }
            }
        }

        function bad() -> Decoder | null {
            return StatusDecoder {}
        }
        "#,
        "E0001",
    );
}

#[test]
fn required_parent_associated_binding_controls_member_type() {
    assert_zero_compile_errors(
        r#"
        interface Parent {
            type Item

            function get(self) -> Self.Item throws never
        }

        interface Child requires Parent<Item = int> {}

        class GoodChild {
            implements Parent {
                type Item = int

                function get(self) -> int {
                    return 1
                }
            }

            implements Child {}
        }

        function use_child(child: Child) -> int {
            return child.get()
        }
        "#,
    );
}

#[test]
fn class_required_parent_must_match_associated_binding() {
    assert_compile_error_contains(
        r#"
        interface Parent {
            type Item

            function get(self) -> Self.Item throws never
        }

        interface Child requires Parent<Item = int> {}

        class BadChild {
            implements Parent {
                type Item = string

                function get(self) -> string {
                    return "bad"
                }
            }

            implements Child {}
        }
        "#,
        "Parent<Item = int>",
    );
}

#[test]
fn qualified_projection_from_child_to_bound_parent_compiles() {
    assert_zero_compile_errors(
        r#"
        interface Parent<T> {
            type Item = T
        }

        interface Child requires Parent<int> {}

        type Projected = (Child as Parent<int>).Item

        function use_projected(value: Projected) -> int {
            return value
        }
        "#,
    );
}

#[test]
fn unique_inherited_typevar_projection_unifies_with_parent_slot() {
    assert_zero_compile_errors(
        r#"
        interface Iterator {
            type Item

            function next(self) -> Self.Item? throws never
        }

        interface Sorted requires Iterator {}

        function head<S extends Sorted>(stream: S) -> S.Item? {
            return stream.next()
        }
        "#,
    );
}

#[test]
fn nested_associated_projection_resolves_after_inner_projection() {
    assert_zero_compile_errors(
        r#"
        interface InnerCarrier {
            type Inner
        }

        class IntInner {
            implements InnerCarrier {
                type Inner = int
            }
        }

        interface Holder {
            type Item extends InnerCarrier
        }

        class IntHolder {
            implements Holder {
                type Item = IntInner
            }
        }

        type Nested = IntHolder.Item.Inner

        function use_nested(value: Nested) -> int {
            return value
        }
        "#,
    );
}

#[test]
fn unknown_projection_on_interface_errors() {
    assert_compile_error_contains(
        r#"
        interface Iterator {
            type Item
        }

        type Missing = Iterator.Element
        "#,
        "unknown associated type `Element`",
    );
}

#[test]
fn unknown_projection_on_interface_alias_errors() {
    assert_compile_error_contains(
        r#"
        interface Iterator {
            type Item
        }

        type IntIterator = Iterator<Item = int>
        type Missing = IntIterator.Element
        "#,
        "unknown associated type `Element`",
    );
}

#[test]
fn unknown_projection_on_typevar_bound_errors() {
    assert_compile_error_contains(
        r#"
        interface Iterator {
            type Item
        }

        function bad<T extends Iterator>(value: T) -> T.Element? {
            return null
        }
        "#,
        "unknown associated type `Element`",
    );
}

#[test]
fn interface_destructure_head_accepts_associated_bindings() {
    assert_zero_compile_errors(
        r#"
        interface Source {
            type Item

            value: Self.Item
        }

        class IntSource {
            value: int

            implements Source {
                type Item = int
            }
        }

        function read(source: Source<Item = int>) -> int {
            return match (source) {
                Source<Item = int> { value } => value
            }
        }
        "#,
    );
}

#[test]
fn interface_destructure_head_associated_binding_controls_field_type() {
    assert_compile_error_code(
        r#"
        interface Source {
            type Item

            value: Self.Item
        }

        function read(source: Source<Item = int> | Source<Item = string>) -> int {
            return match (source) {
                Source<Item = string> { value } => value,
                _ => 0,
            }
        }
        "#,
        "E0001",
    );
}

#[test]
fn associated_interface_alias_projection_compiles() {
    assert_zero_compile_errors(
        r#"
        interface Source {
            type Item

            function get(self) -> Self.Item throws never
        }

        type IntSource = Source<Item = int>

        function read(source: IntSource) -> IntSource.Item {
            return source.get()
        }
        "#,
    );
}

#[test]
fn associated_interface_optional_match_requires_null_arm() {
    assert_compile_error_code(
        r#"
        interface Iterator {
            type Item

            function next(self) -> Self.Item? throws never
        }

        function label(it: Iterator<Item = int>?) -> string {
            return match (it) {
                let ints: Iterator<Item = int> => "int",
            }
        }
        "#,
        "E0062",
    );
}

#[test]
fn associated_union_pattern_does_not_exhaust_wider_associated_binding() {
    assert_compile_error_code(
        r#"
        interface Iterator {
            type Item

            function next(self) -> Self.Item throws never
        }

        function label(it: Iterator<Item = int | string>) -> string {
            return match (it) {
                let ints: Iterator<Item = int> => "int",
                let strings: Iterator<Item = string> => "string",
            }
        }
        "#,
        "E0062",
    );
}

#[tokio::test]
#[ignore = "Runtime match guards do not yet handle a type variable in the requested associated-type pin (`Source<Item = T>`), so the guard over-matches. Compiler-side typing is correct; un-ignore when the runtime guard-template supports typevar pins."]
async fn runtime_guard_accepts_generic_requested_associated_type_var() {
    // The parameter pins both admissible realizations (an existential value type
    // must pin its associated types); the *type pattern* then requests the pin at
    // the function's own generic (`Source<Item = T>`), so the runtime guard
    // filters by the realized binding: the int realization matches at
    // `score<int>`, the string one falls through.
    let output = baml_test!(
        r#"
        interface Source {
            type Item
        }

        class IntSource {
            implements Source {
                type Item = int
            }
        }

        class StringSource {
            implements Source {
                type Item = string
            }
        }

        function score<T>(source: Source<Item = T> | Source<Item = string>) -> int {
            return match (source) {
                let matching: Source<Item = T> => 1,
                _ => 0,
            }
        }

        function main() -> int {
            return score<int>(IntSource {}) * 10 + score<int>(StringSource {})
        }
        "#
    );
    assert_eq!(output.result.unwrap(), BexExternalValue::Int(10));
}

#[tokio::test]
async fn runtime_dispatch_substitutes_class_typevar_in_associated_type_binding() {
    let output = baml_test!(
        r#"
        interface Container {
            type Item

            function get(self) -> Self.Item throws never
        }

        class Box<T> {
            value: T

            implements Container {
                type Item = T

                function get(self) -> T {
                    return self.value
                }
            }
        }

        function unwrap(c: Container<Item = int>) -> int {
            return c.get()
        }

        function main() -> int {
            return unwrap(Box<int> { value: 42 })
        }
        "#
    );
    assert_eq!(output.result.unwrap(), BexExternalValue::Int(42));
}

#[tokio::test]
async fn runtime_dispatch_substitutes_class_typevar_inside_nested_associated_binding() {
    let output = baml_test!(
        r#"
        interface Source {
            type Item

            function get(self) -> Self.Item throws never
        }

        interface Outer {
            type Inner

            function inner(self) -> Self.Inner throws never
        }

        class Box<T> {
            value: T

            implements Source {
                type Item = T

                function get(self) -> T {
                    return self.value
                }
            }
        }

        class OuterBox<T> {
            source: Source<Item = T>

            implements Outer {
                type Inner = Source<Item = T>

                function inner(self) -> Source<Item = T> {
                    return self.source
                }
            }
        }

        function unwrap(outer: Outer<Inner = Source<Item = int>>) -> int {
            return outer.inner().get()
        }

        function main() -> int {
            return unwrap(OuterBox<int> { source: Box<int> { value: 42 } })
        }
        "#
    );
    assert_eq!(output.result.unwrap(), BexExternalValue::Int(42));
}

#[tokio::test]
async fn runtime_match_filters_by_associated_type_binding() {
    let output = baml_test!(
        r#"
        interface Source {
            type Item

            function get(self) -> Self.Item throws never
        }

        class IntSource {
            value: int

            implements Source {
                type Item = int

                function get(self) -> int {
                    return self.value
                }
            }
        }

        class StringSource {
            value: string

            implements Source {
                type Item = string

                function get(self) -> string {
                    return self.value
                }
            }
        }

        function score(source: Source<Item = int> | Source<Item = string>) -> int {
            return match (source) {
                let ints: Source<Item = int> => ints.get(),
                _ => 0,
            }
        }

        function main() -> int {
            return score(StringSource { value: "no" }) + score(IntSource { value: 7 })
        }
        "#
    );
    assert_eq!(output.result.unwrap(), BexExternalValue::Int(7));
}

#[tokio::test]
async fn runtime_destructure_filters_by_associated_type_binding() {
    let output = baml_test!(
        r#"
        interface Source {
            type Item

            value: Self.Item
        }

        class IntSource {
            value: int

            implements Source {
                type Item = int
            }
        }

        class StringSource {
            value: string

            implements Source {
                type Item = string
            }
        }

        function score(source: Source<Item = int> | Source<Item = string>) -> int {
            return match (source) {
                Source<Item = int> { value } => value,
                _ => 0,
            }
        }

        function main() -> int {
            return score(StringSource { value: "no" }) + score(IntSource { value: 9 })
        }
        "#
    );
    assert_eq!(output.result.unwrap(), BexExternalValue::Int(9));
}

// `Bucket<L>` + `Bucket<R>` on `Pair<L, R>` realize the same interface
// `Bucket<T>` at the diagonal `Pair<T, T>`, so they overlap and are rejected —
// even when the interface and impls live in different namespaces.
#[test]
fn partially_open_associated_binding_overlapping_impls_rejected_across_namespaces() {
    assert_compile_error_code_multi(
        &[
            (
                "ns_contracts/contracts.baml",
                r#"
                interface Bucket<T> {
                    type Shape

                    function get(self) -> string throws never
                }

                interface Routed requires Bucket<Self.Item, Shape = map<Self.Item, int>> {
                    type Item

                    function chosen(self) -> string throws never {
                        return self.get()
                    }
                }
                "#,
            ),
            (
                "ns_models/pair.baml",
                r#"
                class Pair<L, R> {
                    left: L
                    right: R

                    implements root.contracts.Bucket<L> {
                        type Shape = map<L, int>

                        function get(self) -> string {
                            return "left"
                        }
                    }

                    implements root.contracts.Bucket<R> {
                        type Shape = map<R, int>

                        function get(self) -> string {
                            return "right"
                        }
                    }

                    implements root.contracts.Routed {
                        type Item = R
                    }
                }
                "#,
            ),
            (
                "ns_app/main.baml",
                r#"
                function main() -> string {
                    let p: root.models.Pair<int, string> = root.models.Pair { left: 1, right: "two" }
                    let routed: root.contracts.Routed<Item = string> = p
                    return routed.chosen()
                }
                "#,
            ),
        ],
        "E0132",
    );
}

// `Bucket<L>` + `Bucket<R>` on `Pair<L, R>` realize the same interface
// `Bucket<T>` at the diagonal `Pair<T, T>`, so they overlap and are rejected.
#[test]
fn partially_open_associated_binding_overlapping_impls_rejected_structurally() {
    assert_compile_error_code(
        r#"
        interface Bucket<T> {
            type Shape

            function get(self) -> string throws never
        }

        interface Routed requires Bucket<Self.Item, Shape = map<Self.Item, int>> {
            type Item

            function chosen(self) -> string throws never {
                return self.get()
            }
        }

        class Pair<L, R> {
            left: L
            right: R

            implements Bucket<L> {
                type Shape = map<L, int>

                function get(self) -> string {
                    return "left"
                }
            }

            implements Bucket<R> {
                type Shape = map<R, int>

                function get(self) -> string {
                    return "right"
                }
            }

            implements Routed {
                type Item = R
            }
        }

        function main() -> string {
            let p: Pair<int, string> = Pair { left: 1, right: "two" }
            let routed: Routed<Item = string> = p
            return routed.chosen()
        }
        "#,
        "E0132",
    );
}

#[tokio::test]
async fn reflection_implements_respects_associated_type_bindings() {
    let output = baml_test!(
        r#"
        interface Source {
            type Item
        }

        class IntSource {
            implements Source {
                type Item = int
            }
        }

        class StringSource {
            implements Source {
                type Item = string
            }
        }

        function main() -> int {
            let score = 0
            if reflect.type_of<IntSource>().implements(reflect.type_of<Source<Item = int>>()) {
                score = score + 1
            }
            if reflect.type_of<IntSource>().implements(reflect.type_of<Source<Item = string>>()) {
                score = score + 10
            }
            if reflect.type_of<StringSource>().implements(reflect.type_of<Source<Item = string>>()) {
                score = score + 100
            }
            if reflect.type_of<StringSource>().implements(reflect.type_of<Source<Item = int>>()) {
                score = score + 1000
            }
            return score
        }
        "#
    );
    assert_eq!(output.result.unwrap(), BexExternalValue::Int(101));
}

#[tokio::test]
async fn reflection_implementors_respects_associated_type_bindings() {
    let output = baml_test!(
        r#"
        interface Source {
            type Item
        }

        class IntSource {
            implements Source {
                type Item = int
            }
        }

        class StringSource {
            implements Source {
                type Item = string
            }
        }

        function main() -> int {
            let int_impls = reflect.type_of<Source<Item = int>>().implementors()
            let string_impls = reflect.type_of<Source<Item = string>>().implementors()
            let score = 0
            score = score + int_impls.length()
            if int_impls.length() > 0 && int_impls[0] == reflect.type_of<IntSource>() {
                score = score + 10
            }
            score = score + (string_impls.length() * 100)
            if string_impls.length() > 0 && string_impls[0] == reflect.type_of<StringSource>() {
                score = score + 1000
            }
            return score
        }
        "#
    );
    assert_eq!(output.result.unwrap(), BexExternalValue::Int(1111));
}

#[tokio::test]
async fn reflection_resolves_generic_class_associated_type_binding() {
    // A generic class's `type Item = T` binding resolves precisely at the
    // instantiation: `Box<int>` *is* `Source<Item = int>` (consistent with
    // runtime dispatch, which accepts `Box<int>` where `Source<Item = int>` is
    // expected, and with how Rust resolves the projection), but it is *not*
    // `Source<Item = string>`. `implementors()`, however, cannot enumerate
    // generic instantiations, so it lists the generic base `Box` for *any*
    // specific `Source<Item = …>` request (it can't pin the instantiation).
    let output = baml_test!(
        r#"
        interface Source {
            type Item
        }

        class Box<T> {
            value: T

            implements Source {
                type Item = T
            }
        }

        function main() -> int {
            let box_int = reflect.type_of<Box<int>>()
            let int_source = reflect.type_of<Source<Item = int>>()
            let string_source = reflect.type_of<Source<Item = string>>()

            let score = 0
            if box_int.implements(int_source) {
                score = score + 1
            }
            if box_int.implements(string_source) {
                score = score + 10
            }
            if string_source.implemented_by(box_int) {
                score = score + 100
            }
            if string_source.implementors().length() > 0 {
                score = score + 1000
            }
            return score
        }
        "#
    );
    // `Box<int>.implements(Source<Item = int>)` holds (+1); the two `Item = string`
    // membership checks are correctly false; and `implementors()` lists the generic
    // base `Box` for the `Source<Item = string>` request (+1000) since it can't pin
    // the instantiation.
    assert_eq!(output.result.unwrap(), BexExternalValue::Int(1001));
}

#[tokio::test]
async fn reflection_respects_generic_interface_impl_bound() {
    // An impl-level bound that is itself a generic interface carrying an
    // associated binding (`T extends Source<Item = int>`). The runtime resolver
    // must discharge it as a nested obligation *at that exact instantiation*: a
    // type implementing `Source<Item = int>` satisfies the blanket impl, while
    // one implementing `Source<Item = string>` does not.
    let output = baml_test!(
        r#"
        interface Source {
            type Item
        }

        class IntSource {
            implements Source {
                type Item = int
            }
        }

        class StrSource {
            implements Source {
                type Item = string
            }
        }

        interface IntSourced {}
        implement<T extends Source<Item = int>> IntSourced for T {}

        function main() -> int {
            let score = 0
            if reflect.type_of<IntSource>().implements(reflect.type_of<IntSourced>()) {
                score = score + 1
            }
            if reflect.type_of<StrSource>().implements(reflect.type_of<IntSourced>()) {
                score = score + 10
            }
            return score
        }
        "#
    );
    assert_eq!(output.result.unwrap(), BexExternalValue::Int(1));
}

#[tokio::test]
async fn reflection_respects_generic_interface_impl_bound_plain_args() {
    // The plain-args form of a generic-interface bound (`T extends
    // Container<int>`, an interface *type argument* rather than an associated
    // binding). The resolver substitutes the bound's args and requires the
    // type-arg to implement the interface at exactly that instantiation.
    let output = baml_test!(
        r#"
        interface Container<T> {}
        class IntBox {
            implements Container<int> {}
        }
        class StrBox {
            implements Container<string> {}
        }

        interface NeedsIntContainer {}
        implement<T extends Container<int>> NeedsIntContainer for T {}

        function main() -> int {
            let score = 0
            if reflect.type_of<IntBox>().implements(reflect.type_of<NeedsIntContainer>()) {
                score = score + 1
            }
            if reflect.type_of<StrBox>().implements(reflect.type_of<NeedsIntContainer>()) {
                score = score + 10
            }
            return score
        }
        "#
    );
    assert_eq!(output.result.unwrap(), BexExternalValue::Int(1));
}

#[tokio::test]
async fn reflection_literal_type_uses_concrete_base_impls() {
    // A literal type uses its concrete type's impls: `1` is an `int`, so reflection
    // normalizes it to its concrete base before consulting the registry. It must
    // answer exactly as `type_of<int>()` does.
    let output = baml_test!(
        r#"
        interface Debuggable {
            function debug(self) -> string throws never
        }
        implements Debuggable for int {
            function debug(self) -> string { return "int" }
        }

        function main() -> int {
            let score = 0
            if reflect.type_of<1>().implements(reflect.type_of<Debuggable>()) {
                score = score + 1
            }
            if reflect.type_of<int>().implements(reflect.type_of<Debuggable>()) {
                score = score + 10
            }
            return score
        }
        "#
    );
    assert_eq!(output.result.unwrap(), BexExternalValue::Int(11));
}

#[tokio::test]
async fn reflection_enum_variant_type_uses_concrete_base_impls() {
    // `Color.Red` as a type uses `Color`'s impls — reflection normalizes the
    // enum-variant type to its enum base before consulting the registry.
    let output = baml_test!(
        r#"
        interface Named {
            function name(self) -> string throws never
        }
        enum Color { Red  Green }
        implements Named for Color {
            function name(self) -> string { return "color" }
        }

        function main() -> int {
            let score = 0
            if reflect.type_of<Color.Red>().implements(reflect.type_of<Named>()) {
                score = score + 1
            }
            if reflect.type_of<Color>().implements(reflect.type_of<Named>()) {
                score = score + 10
            }
            return score
        }
        "#
    );
    assert_eq!(output.result.unwrap(), BexExternalValue::Int(11));
}

#[tokio::test]
async fn reflection_bounded_impl_cycle_terminates() {
    // Mutually-recursive blanket bounds: `A` is implemented by anything that is
    // `B`, and `B` by anything that is `A`. No concrete type breaks the cycle, so
    // `Node` implements neither — and resolution must *terminate*: the obligation
    // stack detects the `Node: A ⇒ Node: B ⇒ Node: A` cycle and rejects it
    // instead of spinning.
    let output = baml_test!(
        r#"
        interface A {}
        interface B {}
        implement<T extends B> A for T {}
        implement<T extends A> B for T {}

        class Node {}

        function main() -> int {
            let score = 0
            if reflect.type_of<Node>().implements(reflect.type_of<A>()) {
                score = score + 1
            }
            if reflect.type_of<Node>().implements(reflect.type_of<B>()) {
                score = score + 10
            }
            return score
        }
        "#
    );
    assert_eq!(output.result.unwrap(), BexExternalValue::Int(0));
}

/// An interface default method whose generic bound references an associated
/// type via `Self` (`U extends Self.Item`) lowers the bound in the same `Self`
/// scope as the signature — so it resolves to a *projection*, which is then
/// rejected as a non-interface bound (bounds are interfaces only, E0145;
/// Rust-parity: `U: Self::Item` is E0404 "expected trait"). The failure is
/// loud, never an erased-`Self` bound silently dropped from enforcement.
#[test]
fn interface_default_method_self_referencing_bound_is_rejected() {
    assert_compile_error_code(
        r#"
        interface Container {
            type Item
            function first(self) -> Self.Item throws never
            function pick<U extends Self.Item>(self, candidate: U) -> U  throws never {
                return candidate
            }
        }
        "#,
        "E0145",
    );
}

// ---------------------------------------------------------------------------
// Opaque associated-type forwarding through a wrapper (scenario-15 gap).
//
// A wrapper class forwards an interface that carries an opaque associated type
// (`type Transcript`). It must expose its own binding as `type Transcript =
// unknown` and forward `begin -> step -> submit` to an inner provider. The
// transcript leaves `begin` typed `unknown` (sound: an opaque projection widens
// TO the top type) and must be accepted BACK into `step`/`submit`, whose params
// are the inner's opaque `Tools.Transcript`. That `unknown -> opaque
// projection` direction is the fix — the parse<T>-grade trust boundary in this
// dynamic, runtime-checked VM.
// ---------------------------------------------------------------------------

#[test]
fn wrapper_forwards_opaque_associated_type_via_existential_field() {
    assert_zero_compile_errors(
        r#"
        class ToolCalls {}

        interface Tools {
            type Transcript

            function begin(self, prompt: string) -> Transcript
            function step(self, t: Transcript) -> string | ToolCalls
            function submit(self, t: Transcript) -> Transcript
        }

        class Guarded {
            inner: Tools

            implements Tools {
                type Transcript = unknown

                function begin(self, prompt: string) -> unknown {
                    return self.inner.begin(prompt)
                }

                function step(self, t: unknown) -> string | ToolCalls {
                    return self.inner.step(t)
                }

                function submit(self, t: unknown) -> unknown {
                    return self.inner.submit(t)
                }
            }
        }
        "#,
    );
}

#[test]
fn wrapper_forwards_opaque_associated_type_via_match_narrowed_existential() {
    // The scenario-15 shape: `inner: Provider`, narrowed to `Tools` via `match`.
    assert_zero_compile_errors(
        r#"
        class ToolCalls {}

        interface Provider {
            function name(self) -> string
        }

        interface Tools requires Provider {
            type Transcript

            function begin(self, prompt: string) -> Transcript
            function step(self, t: Transcript) -> string | ToolCalls
            function submit(self, t: Transcript) -> Transcript
        }

        class Guarded {
            inner: Provider

            implements Provider {
                function name(self) -> string { return "guarded" }
            }

            implements Tools {
                type Transcript = unknown

                function begin(self, prompt: string) -> unknown {
                    match (self.inner) {
                        let tp: Tools => tp.begin(prompt),
                        _ => "no",
                    }
                }

                function step(self, t: unknown) -> string | ToolCalls {
                    match (self.inner) {
                        let tp: Tools => tp.step(t),
                        _ => "no",
                    }
                }

                function submit(self, t: unknown) -> unknown {
                    match (self.inner) {
                        let tp: Tools => tp.submit(t),
                        _ => t,
                    }
                }
            }
        }
        "#,
    );
}

#[test]
fn unknown_still_rejected_where_concrete_associated_witness_is_expected() {
    // The fix is scoped to OPAQUE projections (existential interface base). A
    // projection over a CONCRETE base resolves to its witness (`int` here), so
    // `unknown` must still be rejected — otherwise the trust boundary would leak
    // into ordinary concrete code.
    assert_compile_error_code(
        r#"
        interface Tools {
            type Transcript

            function step(self, t: Transcript) -> string
        }

        class IntTools {
            implements Tools {
                type Transcript = int

                function step(self, t: int) -> string {
                    return "ok"
                }
            }
        }

        function bad(tools: IntTools, blob: unknown) -> string {
            return tools.step(blob)
        }
        "#,
        "E0001",
    );
}

#[tokio::test]
async fn wrapper_forwards_full_tool_loop_round_trip_through_scripted_fake() {
    // End-to-end: a scripted `Tools` fake owns a real transcript
    // (`FakeTranscript`, a struct); the `Guarded` wrapper forwards
    // begin -> step -> submit -> step, threading the transcript as `unknown`
    // across every method boundary. The driver runs the loop through the
    // wrapper typed as an unbound `Tools` existential, proving the transcript
    // survives the `unknown` trip and lands back in the fake as a real
    // `FakeTranscript` at run time.
    let output = baml_test!(
        r#"
        class ToolCalls {}

        class FakeTranscript {
            steps: int
        }

        interface Tools {
            type Transcript

            function begin(self, prompt: string) -> Transcript
            function step(self, t: Transcript) -> string | ToolCalls
            function submit(self, t: Transcript) -> Transcript
        }

        class FakeProvider {
            implements Tools {
                type Transcript = FakeTranscript

                function begin(self, prompt: string) -> FakeTranscript {
                    return FakeTranscript { steps: 0 }
                }

                function step(self, t: FakeTranscript) -> string | ToolCalls {
                    if t.steps == 0 {
                        return ToolCalls {}
                    }
                    return "final"
                }

                function submit(self, t: FakeTranscript) -> FakeTranscript {
                    return FakeTranscript { steps: t.steps + 1 }
                }
            }
        }

        class Guarded {
            inner: Tools

            implements Tools {
                type Transcript = unknown

                function begin(self, prompt: string) -> unknown {
                    return self.inner.begin(prompt)
                }

                function step(self, t: unknown) -> string | ToolCalls {
                    return self.inner.step(t)
                }

                function submit(self, t: unknown) -> unknown {
                    return self.inner.submit(t)
                }
            }
        }

        function drive(tools: Tools) -> string {
            let t0 = tools.begin("hi")
            return match (tools.step(t0)) {
                let final0: string => "immediate:" + final0,
                let calls: ToolCalls => {
                    let t1 = tools.submit(t0)
                    match (tools.step(t1)) {
                        let final1: string => "via-tools:" + final1,
                        let more: ToolCalls => "stuck",
                    }
                },
            }
        }

        function main() -> string {
            let g = Guarded { inner: FakeProvider {} }
            return drive(g)
        }
        "#
    );

    assert_eq!(
        output.result.unwrap(),
        BexExternalValue::String("via-tools:final".into())
    );
}
