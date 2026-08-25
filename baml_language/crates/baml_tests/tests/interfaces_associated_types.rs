//! Tests for BEP-057 associated types on interfaces.
//!
//! The suite covers declaration/binding syntax, default witnesses, projection
//! disambiguation, required-interface propagation, unions, destructuring, and
//! runtime dispatch through associated interface views.
//!
//! Two tests require isolated Rust compilation because their patterns cause
//! stack overflows in larger projects: `runtime_guard_accepts_generic_requested_associated_type_var`
//! (runtime guard-template does not yet support typevar pins) and
//! `reflection_bounded_impl_cycle_terminates` (mutually-recursive universal
//! blanket impls overflow the compiler stack). The rest covers compile-diagnostics,
//! VM-metadata, and formatter behavior that requires Rust-side infrastructure.

use std::collections::HashSet;

use baml_compiler_diagnostics::Severity;
use baml_fmt::FormatOptions;
use baml_project::ProjectDatabase;
use baml_tests::{
    baml_test,
    engine::{OptLevel, compile_source_with_opt},
    stdlib_prefix::{check_user_files, setup_multi_file_db, setup_test_db},
};
use bex_engine::BexExternalValue;
use bex_vm_types::Object;

fn collect_compile_errors(source: &str) -> Vec<String> {
    let db = setup_test_db(source);
    collect_compile_errors_from_db(&db)
}

fn collect_compile_errors_multi(files: &[(&str, &str)]) -> Vec<String> {
    let db = setup_multi_file_db(files);
    collect_compile_errors_from_db(&db)
}

fn collect_compile_errors_from_db(db: &ProjectDatabase) -> Vec<String> {
    let all_files = db.get_source_files();
    let user_file_ids: HashSet<_> = all_files.iter().map(|f| f.file_id(db)).collect();

    check_user_files(db)
        .into_iter()
        .filter(|d| matches!(d.severity, Severity::Error))
        .filter(|d| {
            d.primary_span()
                .map(|span| user_file_ids.contains(&span.file_id))
                .unwrap_or(false)
        })
        .map(|d| format!("[{}] {}", d.code(), d.message_with_primary_label()))
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
    let heap = baml_tests::engine::bound_pool(&program);
    let ptr = heap.compile_time_ptr(*idx);
    // SAFETY: `ptr` indexes the pool the heap was just built from, and the
    // unsealed heap outlives every read below.
    let Object::Function(function) = (unsafe { ptr.get() }) else {
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
    // are *not* erased: the stored signature is a template over the callee frame, so
    // `T` is carried as the frame slot it occupies (`#0`) and the projection keeps its
    // resolved form — the declaring interface is determined at lowering, which is
    // strictly more precise than the bare `T.Item` for runtime resolution. Naming the
    // slot rather than the variable is what lets a *value* of this function
    // substitute the realized args it carries (see `bex_vm`'s `function_object_ty`).
    assert_eq!(params, vec!["#0"]);
    assert_eq!(return_type, "(#0 as BoxLike).Item");
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

#[test]
fn class_inherent_method_does_not_satisfy_abstract_associated_type_method() {
    // `Ticket`'s `value` is an inherent method (outside the `implements Describable`
    // block, which binds only `type Output`), so it does NOT satisfy the abstract
    // `Describable.value` (BEP-044: only `implements`-block members satisfy a
    // requirement) → E0113.
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
    // A child interface (`Cursor requires It`) whose default method returns the
    // inherited associated type in a scalar/optional position (`Self.Item?`) and
    // delegates through `self.next()`. The inherited `Item` projects onto the rigid
    // `Self`, matching how `self.next()` resolves it, so the declared return and
    // the body agree.
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

            implements Codec<(value: string) -> int throws never> {
                type Output = bool
            }
        }

        type TextOut = (Document as Codec<TextFormat>).Output
        type MaybeTextOut = (Document as Codec<TextFormat>).Output?
        type TextOutList = (Document as Codec<TextFormat>).Output[]
        type TextOutMap = map<string, (Document as Codec<TextFormat>).Output>
        type MapArgOut = (Document as Codec<map<string, PairFormat[]?>>).Output
        type UnionArgOut = (Document as Codec<(TextFormat | CodeFormat)>).Output
        type FunctionArgOut = (Document as Codec<(value: string) -> int throws never>).Output
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
            value: (x: int) -> int throws never

            implements Source {
                type Item = (x: int) -> int throws never

                function get(self) -> (x: int) -> int throws never {
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

        function read_callback<T, S extends Source<Item = (x: T) -> T throws never>>(source: S) -> (x: T) -> T throws never {
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

        function use_callback() -> (x: int) -> int throws never {
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
fn unbound_associated_interface_scrutinee_requires_binding() {
    assert_compile_error_contains(
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
        "must specify its associated type(s) `Item`",
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

            function lift(self) -> ((Self.Item) -> Self.Item? throws never) throws never
        }

        function lift_int(lifter: Lifter<Item = int>) -> (int) -> int? throws never {
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
        "cannot project `Element` directly off interface `Iterator`",
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
        "cannot project `Element` directly off interface `Iterator`",
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
    // The one interface-headed base the projection shorthand accepts: an
    // alias whose written spelling pins the projected member — the
    // projection collapses to the pin, no implementor needed. (A bare
    // interface base is rejected — see
    // `unknown_projection_on_interface_errors`.)
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
fn narrower_associated_interface_pattern_is_rejected() {
    assert_compile_error_contains(
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
        "mismatched types: expected `Iterator<Item = int | string>`, found `Iterator<Item = int>`",
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
            if reflect.Type.of<Node>().implements(reflect.Type.of<A>()) {
                score = score + 1
            }
            if reflect.Type.of<Node>().implements(reflect.Type.of<B>()) {
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
