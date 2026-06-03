//! Tests for BEP-057 associated types on interfaces.
//!
//! The suite covers declaration/binding syntax, default witnesses, projection
//! disambiguation, required-interface propagation, unions, destructuring, and
//! runtime dispatch through associated interface views.

use std::collections::HashSet;

use baml_compiler_diagnostics::Severity;
use baml_fmt::FormatOptions;
use baml_project::{ProjectDatabase, collect_diagnostics, testing::setup_test_db};
use baml_tests::baml_test;
use bex_engine::BexExternalValue;

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
fn assert_compile_error_contains(source: &str, needle: &str) {
    let errors = collect_compile_errors(source);
    assert!(
        errors.iter().any(|error| error.contains(needle)),
        "expected compile error containing `{needle}`, got:\n  {}",
        errors.join("\n  ")
    );
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

            function neighbors(self, node: Node) -> Node[]
            function edge_label(self, edge: Edge) -> LabelType
            function weight(self, edge: Edge) -> Weight
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
fn associated_type_bindings_substitute_inside_implements_blocks() {
    assert_zero_compile_errors(
        r#"
        interface Stack {
            type Item

            function push(self, value: Item) -> null
            function peek(self) -> Self.Item?
            function pair(self, value: Item) -> Self.Item[] {
                return [value, value]
            }
        }

        class IntStack {
            implements Stack {
                type Item = int

                function push(self, value: Item) -> null {
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
            function next(self) -> Self.Item?
            function firstOrNull(self) -> Self.Item? {
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
fn fully_bound_associated_type_interface_values_expose_projected_methods() {
    assert_zero_compile_errors(
        r#"
        interface Iterator {
            type Item

            function next(self) -> Item?
            function size(self) -> int
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

            function next(self) -> Item?
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
fn generic_bound_enforces_associated_type_binding() {
    assert_compile_error_code(
        r#"
        interface Iterator {
            type Item

            function next(self) -> Item?
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

            function next(self) -> Item?
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

            function parse(self, input: string) -> Output
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
fn required_parent_associated_type_threads_into_child_interface() {
    assert_zero_compile_errors(
        r#"
        interface Iterator {
            type Item

            function next(self) -> Item?
        }

        interface Sorted requires Iterator {
            function sorted(self) -> Self.Item[]
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

            function next(self) -> Item?
        }

        interface Reader {
            type Item

            function read(self) -> Item
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

            function next(self) -> Item?
        }

        interface Reader {
            type Item

            function read(self) -> Item
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

            function decode(self, input: string) -> Output
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
            return doc.as<Codec<TextFormat>>.decode("")
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
fn generic_interface_associated_type_bindings_compile() {
    assert_zero_compile_errors(
        r#"
        interface Cache<K> {
            type Value

            function get(self, key: K) -> Value?
            function put(self, key: K, value: Value) -> null
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

            function next(self) -> Item?
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

            function decode(self, input: string) -> Output
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
            return doc.as<Codec<TextFormat>>.decode("")
        }

        function decode_code(doc: Document) -> (Document as Codec<CodeFormat>).Output {
            return doc.as<Codec<CodeFormat>>.decode("")
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

            function get(self) -> Item
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
            type Items = Item[]

            function all(self) -> Items
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

                function get(self) -> Value
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
fn explicit_associated_type_witness_can_reference_earlier_witness() {
    assert_zero_compile_errors(
        r#"
        interface Batch {
            type Item
            type Items

            function all(self) -> Items
        }

        class IntBatch {
            values: int[]

            implements Batch {
                type Item = int
                type Items = Item[]

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
fn dependent_associated_type_bound_uses_resolved_witness() {
    assert_zero_compile_errors(
        r#"
        interface Parser {
            type Item
            type Output extends Item

            function parse(self) -> Output
        }

        class IntParser {
            implements Parser {
                type Item = int
                type Output = int

                function parse(self) -> int {
                    return 1
                }
            }
        }

        function parse(parser: Parser<Item = int, Output = int>) -> int {
            return parser.parse()
        }
        "#,
    );
}

#[test]
fn dependent_associated_type_bound_rejects_mismatched_interface_binding() {
    assert_compile_error_contains(
        r#"
        interface Parser {
            type Item
            type Output extends Item
        }

        function bad(parser: Parser<Item = int, Output = string>) -> null {
            return null
        }
        "#,
        "does not satisfy bound",
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

            function decode(self, input: string) -> Output
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

            function decode(self, input: string) -> Output
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

            function decode(self, input: string) -> Output
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

            function get(self) -> Item
            function maybe(self) -> Item?
            function either(self) -> Item | bool
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

            function payload(self) -> Payload
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
fn associated_type_bounds_can_be_union_types() {
    assert_zero_compile_errors(
        r#"
        interface Parser {
            type Output extends int | string

            function parse(self) -> Output
        }

        class IntParser {
            implements Parser {
                type Output = int

                function parse(self) -> int {
                    return 42
                }
            }
        }

        class StringParser {
            implements Parser {
                type Output = string

                function parse(self) -> string {
                    return "ok"
                }
            }
        }

        function parse_int(parser: IntParser) -> IntParser.Output {
            return parser.parse()
        }

        function parse_bound<P extends Parser>(parser: P) -> P.Output {
            return parser.parse()
        }
        "#,
    );
}

#[test]
fn union_associated_type_bindings_work_in_generic_bounds() {
    assert_zero_compile_errors(
        r#"
        interface Source {
            type Item

            function get(self) -> Item
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
fn qualified_projections_disambiguate_union_outputs() {
    assert_zero_compile_errors(
        r#"
        class TextFormat {}
        class CodeFormat {}

        interface Codec<Format> {
            type Output

            function decode(self, input: string) -> Output
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
            return doc.as<Codec<TextFormat>>.decode("")
        }

        function decode_any(doc: Document) ->
            (Document as Codec<TextFormat>).Output | (Document as Codec<CodeFormat>).Output {
            return doc.as<Codec<CodeFormat>>.decode("")
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

            function decode(self, input: string) -> Output
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
                return doc.as<Codec<TextFormat>>.decode("")
            }
            return doc.as<Codec<CodeFormat>>.decode("")
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

            function decode(self, input: string) -> Output
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
            return doc.as<Codec<TextFormat>>.decode("")
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

            function next(self) -> Item
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

            function next(self) -> Item
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

            function next(self) -> Item
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

            value: Item
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

            function list(self) -> Item[]
            function table(self) -> map<string, Item | null>
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

            function decode(self, input: string) -> Output
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

            function get(self) -> Item
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

            function repr(self) -> Repr
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

            function next(self) -> Item?
        }

        class IntIterator {}

        implements Iterator<Item = int> for IntIterator {
            type Item = int

            function next(self) -> int? {
                return null
            }
        }
        "#,
        "associated type bindings are not allowed in `implements` targets",
    );
}

#[test]
fn concrete_class_associated_projection_accepts_bound_type() {
    assert_zero_compile_errors(
        r#"
        interface Carrier {
            type Item

            function get(self) -> Item
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

            function next(self) -> Item?
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

            function next(self) -> Item?
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

            function list(self) -> Item[]
            function table(self) -> map<string, Item?>
            function choose(self) -> Item?
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

            function lift(self) -> (Item) -> Item?
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

            function decode(self, input: string) -> Output
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
            return [doc.as<Codec<TextFormat>>.decode("")]
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

            function decode(self, input: string) -> Output
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
            let output: (Document as Codec<TextFormat>).Output = doc.as<Codec<TextFormat>>.decode("")
            return output
        }
        "#,
    );
}

#[test]
fn unbound_interface_can_call_methods_that_do_not_mention_associated_types() {
    assert_zero_compile_errors(
        r#"
        interface SizedIterator {
            type Item

            function size(self) -> int
            function next(self) -> Item?
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
    );
}

#[test]
fn unbound_interface_projection_can_remain_symbolic() {
    assert_zero_compile_errors(
        r#"
        interface SizedIterator {
            type Item

            function next(self) -> Item?
        }

        function next(it: SizedIterator) -> SizedIterator.Item? {
            return it.next()
        }
        "#,
    );
}

#[test]
fn unbound_interface_method_that_returns_associated_type_errors() {
    assert_compile_error_code(
        r#"
        interface Iterator {
            type Item

            function next(self) -> Item?
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

            function get(self) -> Item
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

            function lift(self) -> (Item) -> Item?
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

            function next(self) -> Item?
        }

        class BadIterator {
            implements Iterator {
                function next(self) -> int? {
                    return null
                }
            }
        }
        "#,
        "does not match interface",
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
    assert_compile_error_code(
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
        "E0012",
    );
}

#[test]
fn implements_target_associated_type_binding_errors() {
    assert_compile_error_contains(
        r#"
        interface Iterator {
            type Item

            function next(self) -> Item?
        }

        class IntIterator {
            implements Iterator<Item = int> {
                function next(self) -> int? {
                    return null
                }
            }
        }
        "#,
        "associated type bindings are not allowed in `implements` targets",
    );
}

#[test]
fn implements_target_associated_type_binding_errors_even_when_same_witness_is_in_body() {
    assert_compile_error_contains(
        r#"
        interface Iterator {
            type Item

            function next(self) -> Item?
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
        "associated type bindings are not allowed in `implements` targets",
    );
}

#[test]
fn impl_associated_type_witness_rejects_extends_bound() {
    assert_compile_error_contains(
        r#"
        interface Iterator {
            type Item

            function next(self) -> Item?
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

            function next(self) -> Item?
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

            function next(self) -> Item?
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
        "associated type bindings are not allowed in `implements` targets",
    );
}

#[test]
fn duplicate_associated_type_binding_on_interface_value_errors() {
    assert_compile_error_contains(
        r#"
        interface Iterator {
            type Item

            function next(self) -> Item?
        }

        function bad(it: Iterator<Item = int, Item = string>) -> int {
            return 0
        }
        "#,
        "Duplicate associated type binding",
    );
}

#[test]
fn duplicate_associated_type_binding_on_interface_value_with_union_errors() {
    assert_compile_error_contains(
        r#"
        interface Iterator {
            type Item

            function next(self) -> Item
        }

        function bad(it: Iterator<Item = int | string, Item = string>) -> int {
            return 0
        }
        "#,
        "Duplicate associated type binding",
    );
}

#[test]
fn unknown_associated_type_binding_in_implements_errors() {
    assert_compile_error_code(
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
        "E0002",
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

            function parse(self) -> Output
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
fn associated_type_union_bound_failure_errors() {
    assert_compile_error_code(
        r#"
        interface Parser {
            type Output extends int | string

            function parse(self) -> Output
        }

        class BadParser {
            implements Parser {
                type Output = bool

                function parse(self) -> bool {
                    return true
                }
            }
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

            function next(self) -> Item
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

            function next(self) -> Item
        }

        class MixedIterator {
            value: int | string

            implements Iterator {
                type Item = int | string

                function next(self) -> Item {
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

#[test]
fn associated_union_required_method_impl_must_match_interface_signature() {
    assert_compile_error_contains(
        r#"
        interface Producer {
            type Item

            function produce(self) -> Item | string
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
        "does not match interface",
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

            function parse(self) -> Output
        }

        function bad<P extends Parser<Output = Label | int>>(parser: P) -> P.Output {
            return parser.parse()
        }
        "#,
        "does not satisfy bound",
    );
}

#[test]
fn unknown_associated_type_binding_on_interface_value_errors() {
    assert_compile_error_contains(
        r#"
        interface Iterator {
            type Item

            function next(self) -> Item?
        }

        function bad(it: Iterator<Element = int>) -> int {
            return 0
        }
        "#,
        "unknown associated type `Element`",
    );
}

#[test]
fn unknown_associated_type_binding_on_interface_value_with_union_errors() {
    assert_compile_error_contains(
        r#"
        interface Iterator {
            type Item

            function next(self) -> Item
        }

        function bad(it: Iterator<Element = int | string, Item = int>) -> int {
            return 0
        }
        "#,
        "unknown associated type `Element`",
    );
}

#[test]
fn ambiguous_unqualified_associated_type_projection_errors() {
    assert_compile_error_code(
        r#"
        interface Iterator {
            type Item

            function next(self) -> Item?
        }

        interface Reader {
            type Item

            function read(self) -> Item
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

            function decode(self, input: string) -> Output
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
            return doc.as<Codec<TextFormat>>.decode("")
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
        "ambiguous associated type projection",
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

            function decode(self, input: string) -> Output
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

            function decode(self, input: string) -> Output
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

            function decode(self, input: string) -> Output
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

            function decode(self, input: string) -> Output
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

            function get(self) -> Item
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

            function get(self) -> Item
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

            function next(self) -> Item?
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

            value: Item
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

            value: Item
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

            function get(self) -> Item
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

            function next(self) -> Item?
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

            function next(self) -> Item
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
async fn runtime_guard_accepts_generic_requested_associated_type_var() {
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

        function score<T>(source: Source) -> int {
            return match (source) {
                let matching: Source<Item = T> => 1,
                _ => 0,
            }
        }

        function main() -> int {
            return score<int>(IntSource {})
        }
        "#
    );
    assert_eq!(output.result.unwrap(), BexExternalValue::Int(1));
}

#[tokio::test]
async fn runtime_match_filters_by_associated_type_binding() {
    let output = baml_test!(
        r#"
        interface Source {
            type Item

            function get(self) -> Item
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

            value: Item
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
async fn reflection_does_not_wildcard_missing_associated_type_bindings() {
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
    assert_eq!(output.result.unwrap(), BexExternalValue::Int(0));
}
