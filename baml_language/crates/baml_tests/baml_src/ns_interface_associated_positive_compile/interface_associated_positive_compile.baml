// Positive associated-type interface fixtures migrated from interfaces_associated_types.rs.
// Isolated reflected packages preserve the original fixture boundaries.

interface CycleA {}

interface CycleB {}

implement<T extends CycleB> CycleA for T {}

implement<T extends CycleA> CycleB for T {}

class CycleNode {
}

test "reflection_bounded_impl_cycle_terminates" {
    assert.is_true(!reflect.Type.of<CycleNode>().implements(reflect.Type.of<CycleA>()));
    assert.is_true(!reflect.Type.of<CycleNode>().implements(reflect.Type.of<CycleB>()))
}

test "implementor_self_coerces_to_interface_with_associated_bindings_in_return_context" {
    root.compiler.AssertCompiles(
        {
            "implementor_self_coerces_to_interface_with_associated_bindings_in_return_context.baml": `
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
            `,
        },
    )
}

test "default_interface_method_can_thread_associated_error_through_callback" {
    root.compiler.AssertCompiles(
        {
            "default_interface_method_can_thread_associated_error_through_callback.baml": `
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
            `,
        },
    )
}

test "associated_type_adapter_class_with_never_error_coerces_to_iterator" {
    root.compiler.AssertCompiles(
        {
            "associated_type_adapter_class_with_never_error_coerces_to_iterator.baml": `
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
            `,
        },
    )
}

test "default_iterator_adapter_method_returns_adapter_with_symbolic_error_projection" {
    root.compiler.AssertCompiles(
        {
            "default_iterator_adapter_method_returns_adapter_with_symbolic_error_projection.baml": `
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
            `,
        },
    )
}

test "associated_type_bindings_substitute_inside_implements_blocks" {
    root.compiler.AssertCompiles(
        {
            "associated_type_bindings_substitute_inside_implements_blocks.baml": `
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
            `,
        },
    )
}

test "inherited_scalar_default_method_delegating_to_self_compiles" {
    root.compiler.AssertCompiles(
        {
            "inherited_scalar_default_method_delegating_to_self_compiles.baml": `
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
            `,
        },
    )
}

test "blanket_impl_binds_associated_type_in_default_body" {
    root.compiler.AssertCompiles(
        {
            "blanket_impl_binds_associated_type_in_default_body.baml": `
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
            `,
        },
    )
}

test "required_parent_associated_type_threads_into_child_interface" {
    root.compiler.AssertCompiles(
        {
            "required_parent_associated_type_threads_into_child_interface.baml": `
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
            `,
        },
    )
}

test "qualified_typevar_projection_disambiguates_required_interfaces" {
    root.compiler.AssertCompiles(
        {
            "qualified_typevar_projection_disambiguates_required_interfaces.baml": `
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
            `,
        },
    )
}

test "generic_interface_associated_type_bindings_compile" {
    root.compiler.AssertCompiles(
        {
            "generic_interface_associated_type_bindings_compile.baml": `
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
            `,
        },
    )
}

test "generic_associated_type_bindings_infer_through_params_and_lets" {
    root.compiler.AssertCompiles(
        {
            "generic_associated_type_bindings_infer_through_params_and_lets.baml": `
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
            `,
        },
    )
}

test "associated_type_default_can_reference_explicit_witness" {
    root.compiler.AssertCompiles(
        {
            "associated_type_default_can_reference_explicit_witness.baml": `
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
            `,
        },
    )
}

test "associated_type_default_from_qualified_interface_resolves_declaring_namespace" {
    root.compiler.AssertCompiles(
        {
            "ns_contracts/contracts.baml": `
                class DefaultValue {
                    value: int
                }

                interface Cache {
                    type Value = DefaultValue

                    function get(self) -> Self.Value throws never
                }
            `,
            "ns_models/models.baml": `
                class LocalCache {
                    implements root.contracts.Cache {
                        function get(self) -> root.contracts.DefaultValue {
                            return root.contracts.DefaultValue { value: 1 }
                        }
                    }
                }
            `,
        },
    )
}

test "associated_type_default_can_reference_declaring_namespace_type_from_implementor_namespace" {
    root.compiler.AssertCompiles(
        {
            "ns_lib/types.baml": `
                interface Serializable {
                    type Format = Payload

                    function serialize(self) -> Self.Format throws never
                }

                class Payload {
                    data: string
                }
            `,
            "ns_app/widget.baml": `
                class Widget {
                    name: string

                    implements root.lib.Serializable {
                        function serialize(self) -> Self.Format {
                            return root.lib.Payload { data: self.name }
                        }
                    }
                }
            `,
        },
    )
}

test "dependent_associated_type_bound_through_interface_resolves_self" {
    root.compiler.AssertCompiles(
        {
            "dependent_associated_type_bound_through_interface_resolves_self.baml": `
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
            `,
        },
    )
}

test "associated_type_binding_order_is_not_semantic" {
    root.compiler.AssertCompiles(
        {
            "associated_type_binding_order_is_not_semantic.baml": `
                interface Pair {
                    type Left
                    type Right
                }

                function reorder(pair: Pair<Left = int, Right = string>) -> Pair<Right = string, Left = int> {
                    return pair
                }
            `,
        },
    )
}

test "mixed_generic_args_and_associated_type_bindings_compile" {
    root.compiler.AssertCompiles(
        {
            "mixed_generic_args_and_associated_type_bindings_compile.baml": `
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
            `,
        },
    )
}

test "named_associated_type_bindings_are_whitespace_insensitive" {
    root.compiler.AssertCompiles(
        {
            "named_associated_type_bindings_are_whitespace_insensitive.baml": `
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
            `,
        },
    )
}

test "associated_type_bindings_allow_trailing_commas_in_type_args" {
    root.compiler.AssertCompiles(
        {
            "associated_type_bindings_allow_trailing_commas_in_type_args.baml": `
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
            `,
        },
    )
}

test "associated_type_binding_can_use_qualified_projection" {
    root.compiler.AssertCompiles(
        {
            "associated_type_binding_can_use_qualified_projection.baml": `
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
            `,
        },
    )
}

test "complex_qualified_projection_type_args_compile" {
    root.compiler.AssertCompiles(
        {
            "complex_qualified_projection_type_args_compile.baml": `
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
            `,
        },
    )
}

test "associated_type_bindings_can_be_union_types" {
    root.compiler.AssertCompiles(
        {
            "associated_type_bindings_can_be_union_types.baml": `
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
            `,
        },
    )
}

test "associated_type_union_defaults_can_reference_interface_generics" {
    root.compiler.AssertCompiles(
        {
            "associated_type_union_defaults_can_reference_interface_generics.baml": `
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
            `,
        },
    )
}

test "union_associated_type_bindings_work_in_generic_bounds" {
    root.compiler.AssertCompiles(
        {
            "union_associated_type_bindings_work_in_generic_bounds.baml": `
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
            `,
        },
    )
}

test "associated_type_binding_in_generic_bound_preserves_outer_typevar" {
    root.compiler.AssertCompiles(
        {
            "associated_type_binding_in_generic_bound_preserves_outer_typevar.baml": `
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
            `,
        },
    )
}

test "union_associated_type_binding_in_generic_bound_preserves_outer_typevar" {
    root.compiler.AssertCompiles(
        {
            "union_associated_type_binding_in_generic_bound_preserves_outer_typevar.baml": `
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
            `,
        },
    )
}

test "nested_associated_type_bindings_in_generic_bounds_preserve_outer_typevar" {
    root.compiler.AssertCompiles(
        {
            "nested_associated_type_bindings_in_generic_bounds_preserve_outer_typevar.baml": `
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
            `,
        },
    )
}

test "qualified_projections_disambiguate_union_outputs" {
    root.compiler.AssertCompiles(
        {
            "qualified_projections_disambiguate_union_outputs.baml": `
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
            `,
        },
    )
}

test "aliased_qualified_projection_unions_resolve_like_inline_unions" {
    root.compiler.AssertCompiles(
        {
            "aliased_qualified_projection_unions_resolve_like_inline_unions.baml": `
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
            `,
        },
    )
}

test "match_narrowing_partitions_interface_associated_bindings_in_union" {
    root.compiler.AssertCompiles(
        {
            "match_narrowing_partitions_interface_associated_bindings_in_union.baml": `
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
            `,
        },
    )
}

test "interface_destructure_substitutes_associated_field_type" {
    root.compiler.AssertCompiles(
        {
            "interface_destructure_substitutes_associated_field_type.baml": `
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
            `,
        },
    )
}

test "associated_type_union_substitutes_inside_nested_containers" {
    root.compiler.AssertCompiles(
        {
            "associated_type_union_substitutes_inside_nested_containers.baml": `
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
            `,
        },
    )
}

test "concrete_class_associated_projection_accepts_bound_type" {
    root.compiler.AssertCompiles(
        {
            "concrete_class_associated_projection_accepts_bound_type.baml": `
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
            `,
        },
    )
}

test "associated_type_projection_expands_class_type_alias_base" {
    root.compiler.AssertCompiles(
        {
            "associated_type_projection_expands_class_type_alias_base.baml": `
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
            `,
        },
    )
}

test "associated_type_projection_expands_fully_bound_interface_alias_base" {
    root.compiler.AssertCompiles(
        {
            "associated_type_projection_expands_fully_bound_interface_alias_base.baml": `
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
            `,
        },
    )
}

test "associated_types_substitute_inside_nested_type_positions" {
    root.compiler.AssertCompiles(
        {
            "associated_types_substitute_inside_nested_type_positions.baml": `
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
            `,
        },
    )
}

test "associated_types_substitute_inside_function_type_positions" {
    root.compiler.AssertCompiles(
        {
            "associated_types_substitute_inside_function_type_positions.baml": `
                interface Lifter {
                    type Item

                    function lift(self) -> ((Self.Item) -> Self.Item? throws never) throws never
                }

                function lift_int(lifter: Lifter<Item = int>) -> (int) -> int? throws never {
                    return lifter.lift()
                }
            `,
        },
    )
}

test "qualified_projection_works_in_nested_types" {
    root.compiler.AssertCompiles(
        {
            "qualified_projection_works_in_nested_types.baml": `
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
            `,
        },
    )
}

test "qualified_projection_works_in_local_type_annotations" {
    root.compiler.AssertCompiles(
        {
            "qualified_projection_works_in_local_type_annotations.baml": `
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
            `,
        },
    )
}

test "associated_type_binding_def_typevar_satisfies_declared_bound" {
    root.compiler.AssertCompiles(
        {
            "associated_type_binding_def_typevar_satisfies_declared_bound.baml": `
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
            `,
        },
    )
}

test "associated_type_interface_binding_typevar_satisfies_declared_bound" {
    root.compiler.AssertCompiles(
        {
            "associated_type_interface_binding_typevar_satisfies_declared_bound.baml": `
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
            `,
        },
    )
}

test "associated_union_required_method_impl_may_narrow_return_covariantly" {
    root.compiler.AssertCompiles(
        {
            "associated_union_required_method_impl_may_narrow_return_covariantly.baml": `
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
            `,
        },
    )
}

test "qualified_projection_from_child_to_bound_parent_compiles" {
    root.compiler.AssertCompiles(
        {
            "qualified_projection_from_child_to_bound_parent_compiles.baml": `
                interface Parent<T> {
                    type Item = T
                }

                interface Child requires Parent<int> {}

                type Projected = (Child as Parent<int>).Item

                function use_projected(value: Projected) -> int {
                    return value
                }
            `,
        },
    )
}

test "unique_inherited_typevar_projection_unifies_with_parent_slot" {
    root.compiler.AssertCompiles(
        {
            "unique_inherited_typevar_projection_unifies_with_parent_slot.baml": `
                interface Iterator {
                    type Item

                    function next(self) -> Self.Item? throws never
                }

                interface Sorted requires Iterator {}

                function head<S extends Sorted>(stream: S) -> S.Item? {
                    return stream.next()
                }
            `,
        },
    )
}

test "nested_associated_projection_resolves_after_inner_projection" {
    root.compiler.AssertCompiles(
        {
            "nested_associated_projection_resolves_after_inner_projection.baml": `
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
            `,
        },
    )
}

test "associated_interface_alias_projection_compiles" {
    root.compiler.AssertCompiles(
        {
            "associated_interface_alias_projection_compiles.baml": `
                interface Source {
                    type Item

                    function get(self) -> Self.Item throws never
                }

                type IntSource = Source<Item = int>

                function read(source: IntSource) -> IntSource.Item {
                    return source.get()
                }
            `,
        },
    )
}
