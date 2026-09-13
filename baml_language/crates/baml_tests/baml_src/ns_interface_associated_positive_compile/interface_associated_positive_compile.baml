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
            "implementor_self_coerces_to_interface_with_associated_bindings_in_return_context.baml": "\n        class Done {}\n\n        interface Iterable {\n            type Item\n            type Error = never\n\n            function iter(self) -> Iterator<Item = Self.Item, Error = Self.Error> throws never\n        }\n\n        interface Iterator requires Iterable<Item = Self.Item, Error = Self.Error> {\n            type Item\n            type Error = never\n\n            function next(self) -> Self.Item | Done throws Self.Error\n        }\n\n        class ArrayIterator<T> {\n            values: T[]\n            idx: int\n\n            implements Iterable {\n                type Item = T\n                type Error = never\n\n                function iter(self) -> Iterator<Item = T, Error = never> throws never {\n                    return self\n                }\n            }\n\n            implements Iterator {\n                type Item = T\n                type Error = never\n\n                function next(self) -> T | Done throws never {\n                    match (self.values.at(self.idx)) {\n                        null => Done {},\n                        let value: T => {\n                            self.idx += 1\n                            value\n                        },\n                    }\n                }\n            }\n        }\n        ",
        },
    )
}

test "default_interface_method_can_thread_associated_error_through_callback" {
    root.compiler.AssertCompiles(
        {
            "default_interface_method_can_thread_associated_error_through_callback.baml": "\n        class Done {}\n\n        interface Iterator {\n            type Item\n            type Error = never\n\n            function next(self) -> Self.Item | Done throws Self.Error\n\n            function map<R, E2>(self, f: (Self.Item) -> R throws E2) -> Mapper<Self.Item, R, Self.Error, E2> throws never {\n                return Mapper<Self.Item, R, Self.Error, E2> { inner: self, f: f }\n            }\n        }\n\n        class Mapper<T, R, E, E2> {\n            inner: Iterator<Item = T, Error = E>\n            f: (T) -> R throws E2\n\n            implements Iterator {\n                type Item = R\n                type Error = E | E2\n\n                function next(self) -> R | Done throws E | E2 {\n                    match (self.inner.next()) {\n                        Done => Done {},\n                        let value: T => self.f(value),\n                    }\n                }\n            }\n        }\n        ",
        },
    )
}

test "associated_type_adapter_class_with_never_error_coerces_to_iterator" {
    root.compiler.AssertCompiles(
        {
            "associated_type_adapter_class_with_never_error_coerces_to_iterator.baml": "\n        class Done {}\n\n        interface Iterable {\n            type Item\n            type Error = never\n\n            function iter(self) -> Iterator<Item = Self.Item, Error = Self.Error> throws never\n        }\n\n        interface Iterator requires Iterable<Item = Self.Item, Error = Self.Error> {\n            type Item\n            type Error = never\n\n            function next(self) -> Self.Item | Done throws Self.Error\n        }\n\n        class Map<T, R, E, E2> {\n            iter: Iterator<Item = T, Error = E>\n            f: (T) -> R throws E2\n\n            implements Iterable {\n                type Item = R\n                type Error = E | E2\n\n                function iter(self) -> Iterator<Item = R, Error = E | E2> throws never {\n                    self\n                }\n            }\n\n            implements Iterator {\n                type Item = R\n                type Error = E | E2\n\n                function next(self) -> R | Done throws E | E2 {\n                    Done {}\n                }\n            }\n        }\n\n        function use_map() -> int {\n            let source: Iterator<Item = int, Error = never> = Map<int, int, never, never> {\n                iter: Empty {},\n                f: (x: int) -> int { x },\n            }\n            let mapped: Iterator<Item = int, Error = never> = Map<int, int, never, never> {\n                iter: source,\n                f: (x: int) -> int { x * 3 },\n            }\n            1\n        }\n\n        class Empty {\n            implements Iterable {\n                type Item = int\n                type Error = never\n\n                function iter(self) -> Iterator<Item = int, Error = never> throws never {\n                    self\n                }\n            }\n\n            implements Iterator {\n                type Item = int\n                type Error = never\n\n                function next(self) -> int | Done throws never {\n                    Done {}\n                }\n            }\n        }\n        ",
        },
    )
}

test "default_iterator_adapter_method_returns_adapter_with_symbolic_error_projection" {
    root.compiler.AssertCompiles(
        {
            "default_iterator_adapter_method_returns_adapter_with_symbolic_error_projection.baml": "\n        class Done {}\n\n        interface Iterable {\n            type Item\n            type Error = never\n\n            function iter(self) -> Iterator<Item = Self.Item, Error = Self.Error> throws never\n        }\n\n        interface Iterator requires Iterable<Item = Self.Item, Error = Self.Error> {\n            type Item\n            type Error = never\n\n            function next(self) -> Self.Item | Done throws Self.Error\n\n            function map<R, E2>(self, f: (Self.Item) -> R throws E2) -> Iterator<Item = R, Error = Self.Error | E2> throws never {\n                Map<Self.Item, R, Self.Error, E2> { iter: self, f: f }\n            }\n        }\n\n        class Map<T, R, E, E2> {\n            iter: Iterator<Item = T, Error = E>\n            f: (T) -> R throws E2\n\n            implements Iterable {\n                type Item = R\n                type Error = E | E2\n\n                function iter(self) -> Iterator<Item = R, Error = E | E2> throws never {\n                    self\n                }\n            }\n\n            implements Iterator {\n                type Item = R\n                type Error = E | E2\n\n                function next(self) -> R | Done throws E | E2 {\n                    Done {}\n                }\n            }\n        }\n        ",
        },
    )
}

test "associated_type_bindings_substitute_inside_implements_blocks" {
    root.compiler.AssertCompiles(
        {
            "associated_type_bindings_substitute_inside_implements_blocks.baml": "\n        interface Stack {\n            type Item\n\n            function push(self, value: Self.Item) -> null throws never\n            function peek(self) -> Self.Item? throws never\n            function pair(self, value: Self.Item) -> Self.Item[] throws never {\n                return [value, value]\n            }\n        }\n\n        class IntStack {\n            implements Stack {\n                type Item = int\n\n                function push(self, value: Self.Item) -> null {\n                    return null\n                }\n\n                function peek(self) -> Self.Item? {\n                    return null\n                }\n            }\n        }\n\n        function top(stack: IntStack) -> IntStack.Item? {\n            return stack.peek()\n        }\n        ",
        },
    )
}

test "inherited_scalar_default_method_delegating_to_self_compiles" {
    root.compiler.AssertCompiles(
        {
            "inherited_scalar_default_method_delegating_to_self_compiles.baml": "\n        interface It {\n            type Item\n            function next(self) -> Self.Item? throws never\n        }\n        interface Cursor requires It {\n            function peek(self) -> Self.Item? throws never {\n                return self.next()\n            }\n        }\n        class IntCursor {\n            value int\n            implements It {\n                type Item = int\n                function next(self) -> Self.Item? { return self.value }\n            }\n            implements Cursor {}\n        }\n        ",
        },
    )
}

test "blanket_impl_binds_associated_type_in_default_body" {
    root.compiler.AssertCompiles(
        {
            "blanket_impl_binds_associated_type_in_default_body.baml": "\n        interface Items {\n            type Item\n            function items(self) -> Self.Item[] throws never\n            function firstItems(self) -> Self.Item[] throws never {\n                return self.items()\n            }\n        }\n        class Box<T> {\n            values: T[]\n        }\n        implements<T> Items for Box<T> {\n            type Item = T\n            function items(self) -> Self.Item[] { return self.values }\n        }\n        ",
        },
    )
}

test "required_parent_associated_type_threads_into_child_interface" {
    root.compiler.AssertCompiles(
        {
            "required_parent_associated_type_threads_into_child_interface.baml": "\n        interface Iterator {\n            type Item\n\n            function next(self) -> Self.Item? throws never\n        }\n\n        interface Sorted requires Iterator {\n            function sorted(self) -> Self.Item[] throws never\n        }\n\n        class Ints {\n            items: int[]\n\n            implements Iterator {\n                type Item = int\n\n                function next(self) -> int? {\n                    return null\n                }\n            }\n\n            implements Sorted {\n                function sorted(self) -> int[] {\n                    return self.items\n                }\n            }\n        }\n        ",
        },
    )
}

test "qualified_typevar_projection_disambiguates_required_interfaces" {
    root.compiler.AssertCompiles(
        {
            "qualified_typevar_projection_disambiguates_required_interfaces.baml": "\n        interface Iterator {\n            type Item\n\n            function next(self) -> Self.Item? throws never\n        }\n\n        interface Reader {\n            type Item\n\n            function read(self) -> Self.Item throws never\n        }\n\n        interface Stream requires Iterator, Reader {}\n\n        function head<S extends Stream>(stream: S) -> (S as Iterator).Item? {\n            return stream.next()\n        }\n        ",
        },
    )
}

test "generic_interface_associated_type_bindings_compile" {
    root.compiler.AssertCompiles(
        {
            "generic_interface_associated_type_bindings_compile.baml": "\n        interface Cache<K> {\n            type Value\n\n            function get(self, key: K) -> Self.Value? throws never\n            function put(self, key: K, value: Self.Value) -> null throws never\n        }\n\n        class StringIntCache {\n            implements Cache<string> {\n                type Value = int\n\n                function get(self, key: string) -> int? {\n                    return null\n                }\n\n                function put(self, key: string, value: int) -> null {\n                    return null\n                }\n            }\n        }\n\n        function read<C extends Cache<string>>(cache: C, key: string) -> C.Value? {\n            return cache.get(key)\n        }\n\n        function main() -> int? {\n            return read<StringIntCache>(StringIntCache {}, \"answer\")\n        }\n        ",
        },
    )
}

test "generic_associated_type_bindings_infer_through_params_and_lets" {
    root.compiler.AssertCompiles(
        {
            "generic_associated_type_bindings_infer_through_params_and_lets.baml": "\n        interface Iterator {\n            type Item\n\n            function next(self) -> Self.Item? throws never\n        }\n\n        function take_one<T>(it: Iterator<Item = T>) -> T? {\n            let same: Iterator<Item = T> = it\n            return same.next()\n        }\n\n        function take_int<I extends Iterator<Item = int>>(it: I) -> int? {\n            return it.next()\n        }\n        ",
        },
    )
}

test "associated_type_default_can_reference_explicit_witness" {
    root.compiler.AssertCompiles(
        {
            "associated_type_default_can_reference_explicit_witness.baml": "\n        interface Batch {\n            type Item\n            type Items = Self.Item[]\n\n            function all(self) -> Self.Items throws never\n        }\n\n        class IntBatch {\n            values: int[]\n\n            implements Batch {\n                type Item = int\n\n                function all(self) -> int[] {\n                    return self.values\n                }\n            }\n        }\n\n        function read(batch: Batch<Item = int>) -> int[] {\n            return batch.all()\n        }\n        ",
        },
    )
}

test "associated_type_default_from_qualified_interface_resolves_declaring_namespace" {
    root.compiler.AssertCompiles(
        {
            "ns_contracts/contracts.baml": "\n            class DefaultValue {\n                value: int\n            }\n\n            interface Cache {\n                type Value = DefaultValue\n\n                function get(self) -> Self.Value throws never\n            }\n            ",
            "ns_models/models.baml": "\n            class LocalCache {\n                implements root.contracts.Cache {\n                    function get(self) -> root.contracts.DefaultValue {\n                        return root.contracts.DefaultValue { value: 1 }\n                    }\n                }\n            }\n            ",
        },
    )
}

test "associated_type_default_can_reference_declaring_namespace_type_from_implementor_namespace" {
    root.compiler.AssertCompiles(
        {
            "ns_lib/types.baml": "\n            interface Serializable {\n                type Format = Payload\n\n                function serialize(self) -> Self.Format throws never\n            }\n\n            class Payload {\n                data: string\n            }\n            ",
            "ns_app/widget.baml": "\n            class Widget {\n                name: string\n\n                implements root.lib.Serializable {\n                    function serialize(self) -> Self.Format {\n                        return root.lib.Payload { data: self.name }\n                    }\n                }\n            }\n            ",
        },
    )
}

test "dependent_associated_type_bound_through_interface_resolves_self" {
    root.compiler.AssertCompiles(
        {
            "dependent_associated_type_bound_through_interface_resolves_self.baml": "\n        interface Producer<T> {\n            function make(self) -> T throws never\n        }\n\n        interface Parser {\n            type Item\n            type Output extends Producer<Self.Item>\n\n            function parse(self) -> Self.Output throws never\n        }\n\n        class IntProducer {\n            value: int\n\n            implements Producer<int> {\n                function make(self) -> int {\n                    return self.value\n                }\n            }\n        }\n\n        class IntParser {\n            implements Parser {\n                type Item = int\n                type Output = IntProducer\n\n                function parse(self) -> IntProducer {\n                    return IntProducer { value: 1 }\n                }\n            }\n        }\n\n        function parse(parser: Parser<Item = int, Output = IntProducer>) -> IntProducer {\n            return parser.parse()\n        }\n        ",
        },
    )
}

test "associated_type_binding_order_is_not_semantic" {
    root.compiler.AssertCompiles(
        {
            "associated_type_binding_order_is_not_semantic.baml": "\n        interface Pair {\n            type Left\n            type Right\n        }\n\n        function reorder(pair: Pair<Left = int, Right = string>) -> Pair<Right = string, Left = int> {\n            return pair\n        }\n        ",
        },
    )
}

test "mixed_generic_args_and_associated_type_bindings_compile" {
    root.compiler.AssertCompiles(
        {
            "mixed_generic_args_and_associated_type_bindings_compile.baml": "\n        class TextFormat {}\n\n        interface Codec<Format> {\n            type Output = string\n\n            function decode(self, input: string) -> Self.Output throws never\n        }\n\n        class Document {\n            raw: string\n\n            implements Codec<TextFormat> {\n                type Output = string\n\n                function decode(self, input: string) -> string {\n                    return self.raw\n                }\n            }\n        }\n\n        function decode_value(decoder: Codec<TextFormat, Output = string>) -> string {\n            return decoder.decode(\"\")\n        }\n\n        function decode_generic<D extends Codec<TextFormat, Output = string>>(decoder: D) -> string {\n            return decoder.decode(\"\")\n        }\n\n        function decode_as(doc: Document) -> (Document as Codec<TextFormat, Output = string>).Output {\n            return doc.as<Codec<TextFormat, Output = string>>.decode(\"\")\n        }\n        ",
        },
    )
}

test "named_associated_type_bindings_are_whitespace_insensitive" {
    root.compiler.AssertCompiles(
        {
            "named_associated_type_bindings_are_whitespace_insensitive.baml": "\n        class TextFormat {}\n\n        interface Sink {\n            type Item\n            type Error = string\n        }\n\n        interface FormattedSink<Format> {\n            type Item\n        }\n\n        type Leading = Sink< Item = int>\n        type TwoNamed = Sink<Item = int, Error = string>\n        type TwoNamedNoSpace = Sink<Item=int,Error=string>\n        type PosThen = FormattedSink<TextFormat, Item = int>\n        type PosThenNoSpace = FormattedSink<TextFormat,Item=int>\n        type Nested = Sink<Item = map<string, int[]>>\n        ",
        },
    )
}

test "associated_type_bindings_allow_trailing_commas_in_type_args" {
    root.compiler.AssertCompiles(
        {
            "associated_type_bindings_allow_trailing_commas_in_type_args.baml": "\n        class TextFormat {}\n\n        interface Sink {\n            type Item\n        }\n\n        interface Codec<Format> {\n            type Output\n\n            function decode(self, input: string) -> Self.Output throws never\n        }\n\n        class Document {\n            raw: string\n\n            implements Codec<TextFormat,> {\n                type Output = string\n\n                function decode(self, input: string) -> string {\n                    return self.raw\n                }\n            }\n        }\n\n        type One = Sink<Item = int,>\n        type Mixed = Codec<TextFormat, Output = string,>\n\n        function decode(doc: Document) -> string {\n            return doc.as<Codec<TextFormat, Output = string,>>.decode(\"\")\n        }\n        ",
        },
    )
}

test "associated_type_binding_can_use_qualified_projection" {
    root.compiler.AssertCompiles(
        {
            "associated_type_binding_can_use_qualified_projection.baml": "\n        class TextFormat {}\n\n        interface Codec<Format> {\n            type Output\n        }\n\n        interface Sink {\n            type Item\n        }\n\n        class Document {\n            implements Codec<TextFormat> {\n                type Output = string\n            }\n        }\n\n        type ProjectedSink = Sink<Item = (Document as Codec<TextFormat>).Output>\n        ",
        },
    )
}

test "complex_qualified_projection_type_args_compile" {
    root.compiler.AssertCompiles(
        {
            "complex_qualified_projection_type_args_compile.baml": "\n        class TextFormat {}\n        class CodeFormat {}\n        class PairFormat {}\n\n        interface Codec<Format> {\n            type Output\n        }\n\n        class Document {\n            implements Codec<TextFormat> {\n                type Output = string\n            }\n\n            implements Codec<map<string, PairFormat[]?>> {\n                type Output = string\n            }\n\n            implements Codec<(TextFormat | CodeFormat)> {\n                type Output = int\n            }\n\n            implements Codec<(value: string) -> int throws never> {\n                type Output = bool\n            }\n        }\n\n        type TextOut = (Document as Codec<TextFormat>).Output\n        type MaybeTextOut = (Document as Codec<TextFormat>).Output?\n        type TextOutList = (Document as Codec<TextFormat>).Output[]\n        type TextOutMap = map<string, (Document as Codec<TextFormat>).Output>\n        type MapArgOut = (Document as Codec<map<string, PairFormat[]?>>).Output\n        type UnionArgOut = (Document as Codec<(TextFormat | CodeFormat)>).Output\n        type FunctionArgOut = (Document as Codec<(value: string) -> int throws never>).Output\n        type WrappedBase = ((Document) as Codec<TextFormat>).Output\n        type WrappedInterface = (Document as (Codec<TextFormat>)).Output\n        ",
        },
    )
}

test "associated_type_bindings_can_be_union_types" {
    root.compiler.AssertCompiles(
        {
            "associated_type_bindings_can_be_union_types.baml": "\n        interface Source {\n            type Item\n\n            function get(self) -> Self.Item throws never\n            function maybe(self) -> Self.Item? throws never\n            function either(self) -> Self.Item | bool throws never\n        }\n\n        class MixedSource {\n            implements Source {\n                type Item = int | string\n\n                function get(self) -> int | string {\n                    return 1\n                }\n\n                function maybe(self) -> (int | string)? {\n                    return null\n                }\n\n                function either(self) -> int | string | bool {\n                    return true\n                }\n            }\n        }\n\n        function read(source: Source<Item = int | string>) -> int | string {\n            return source.get()\n        }\n\n        function read_maybe(source: Source<Item = int | string>) -> (int | string)? {\n            return source.maybe()\n        }\n\n        function read_either(source: Source<Item = int | string>) -> int | string | bool {\n            return source.either()\n        }\n        ",
        },
    )
}

test "associated_type_union_defaults_can_reference_interface_generics" {
    root.compiler.AssertCompiles(
        {
            "associated_type_union_defaults_can_reference_interface_generics.baml": "\n        interface Response<T> {\n            type Payload = T | null\n\n            function payload(self) -> Self.Payload throws never\n        }\n\n        class StringResponse {\n            implements Response<string> {\n                function payload(self) -> string | null {\n                    return null\n                }\n            }\n        }\n\n        function read(response: Response<string>) -> string | null {\n            return response.payload()\n        }\n        ",
        },
    )
}

test "union_associated_type_bindings_work_in_generic_bounds" {
    root.compiler.AssertCompiles(
        {
            "union_associated_type_bindings_work_in_generic_bounds.baml": "\n        interface Source {\n            type Item\n\n            function get(self) -> Self.Item throws never\n        }\n\n        class MixedSource {\n            implements Source {\n                type Item = int | string\n\n                function get(self) -> int | string {\n                    return \"value\"\n                }\n            }\n        }\n\n        function consume<S extends Source<Item = int | string>>(source: S) -> int | string {\n            return source.get()\n        }\n\n        function main(source: MixedSource) -> int | string {\n            return consume<MixedSource>(source)\n        }\n        ",
        },
    )
}

test "associated_type_binding_in_generic_bound_preserves_outer_typevar" {
    root.compiler.AssertCompiles(
        {
            "associated_type_binding_in_generic_bound_preserves_outer_typevar.baml": "\n        interface Source {\n            type Item\n\n            function get(self) -> Self.Item throws never\n        }\n\n        class IntSource {\n            value: int\n\n            implements Source {\n                type Item = int\n\n                function get(self) -> int {\n                    return self.value\n                }\n            }\n        }\n\n        function score_bound<T, S extends Source<Item = T>>(source: S) -> T {\n            return source.get()\n        }\n\n        function main() -> int {\n            let source = IntSource { value: 42 }\n            return score_bound<int, IntSource>(source)\n        }\n        ",
        },
    )
}

test "union_associated_type_binding_in_generic_bound_preserves_outer_typevar" {
    root.compiler.AssertCompiles(
        {
            "union_associated_type_binding_in_generic_bound_preserves_outer_typevar.baml": "\n        interface Source {\n            type Item\n\n            function get(self) -> Self.Item throws never\n        }\n\n        class MixedSource {\n            implements Source {\n                type Item = int | string\n\n                function get(self) -> int | string {\n                    return 1\n                }\n            }\n        }\n\n        function score_bound<T, S extends Source<Item = T>>(source: S) -> T {\n            return source.get()\n        }\n\n        function main(source: MixedSource) -> int | string {\n            return score_bound<int | string, MixedSource>(source)\n        }\n        ",
        },
    )
}

test "nested_associated_type_bindings_in_generic_bounds_preserve_outer_typevar" {
    root.compiler.AssertCompiles(
        {
            "nested_associated_type_bindings_in_generic_bounds_preserve_outer_typevar.baml": "\n        interface Source {\n            type Item\n\n            function get(self) -> Self.Item throws never\n        }\n\n        class OptionalIntSource {\n            value: int?\n\n            implements Source {\n                type Item = int?\n\n                function get(self) -> int? {\n                    return self.value\n                }\n            }\n        }\n\n        class IntListSource {\n            value: int[]\n\n            implements Source {\n                type Item = int[]\n\n                function get(self) -> int[] {\n                    return self.value\n                }\n            }\n        }\n\n        class IntMapSource {\n            value: map<string, int>\n\n            implements Source {\n                type Item = map<string, int>\n\n                function get(self) -> map<string, int> {\n                    return self.value\n                }\n            }\n        }\n\n        class IntCallbackSource {\n            value: (x: int) -> int throws never\n\n            implements Source {\n                type Item = (x: int) -> int throws never\n\n                function get(self) -> (x: int) -> int throws never {\n                    return self.value\n                }\n            }\n        }\n\n        function read_optional<T, S extends Source<Item = T?>>(source: S) -> T? {\n            return source.get()\n        }\n\n        function read_list<T, S extends Source<Item = T[]>>(source: S) -> T[] {\n            return source.get()\n        }\n\n        function read_map<T, S extends Source<Item = map<string, T>>>(source: S) -> map<string, T> {\n            return source.get()\n        }\n\n        function read_callback<T, S extends Source<Item = (x: T) -> T throws never>>(source: S) -> (x: T) -> T throws never {\n            return source.get()\n        }\n\n        function id(value: int) -> int {\n            return value\n        }\n\n        function use_optional() -> int? {\n            return read_optional<int, OptionalIntSource>(OptionalIntSource { value: 1 })\n        }\n\n        function use_list() -> int[] {\n            return read_list<int, IntListSource>(IntListSource { value: [2] })\n        }\n\n        function use_map() -> map<string, int> {\n            return read_map<int, IntMapSource>(IntMapSource { value: { \"x\": 3 } })\n        }\n\n        function use_callback() -> (x: int) -> int throws never {\n            return read_callback<int, IntCallbackSource>(IntCallbackSource { value: id })\n        }\n        ",
        },
    )
}

test "qualified_projections_disambiguate_union_outputs" {
    root.compiler.AssertCompiles(
        {
            "qualified_projections_disambiguate_union_outputs.baml": "\n        class TextFormat {}\n        class CodeFormat {}\n\n        interface Codec<Format> {\n            type Output\n\n            function decode(self, input: string) -> Self.Output throws never\n        }\n\n        class Document {\n            raw: string\n\n            implements Codec<TextFormat> {\n                type Output = string | null\n\n                function decode(self, input: string) -> string | null {\n                    return self.raw\n                }\n            }\n\n            implements Codec<CodeFormat> {\n                type Output = int | bool\n\n                function decode(self, input: string) -> int | bool {\n                    return 200\n                }\n            }\n        }\n\n        type AnyDecoded =\n            (Document as Codec<TextFormat>).Output | (Document as Codec<CodeFormat>).Output\n\n        function decode_text(doc: Document) -> (Document as Codec<TextFormat>).Output {\n            return doc.as<Codec<TextFormat, Output = string | null>>.decode(\"\")\n        }\n\n        function decode_any(doc: Document) ->\n            (Document as Codec<TextFormat>).Output | (Document as Codec<CodeFormat>).Output {\n            return doc.as<Codec<CodeFormat, Output = int | bool>>.decode(\"\")\n        }\n        ",
        },
    )
}

test "aliased_qualified_projection_unions_resolve_like_inline_unions" {
    root.compiler.AssertCompiles(
        {
            "aliased_qualified_projection_unions_resolve_like_inline_unions.baml": "\n        class TextFormat {}\n        class CodeFormat {}\n\n        interface Codec<Format> {\n            type Output\n\n            function decode(self, input: string) -> Self.Output throws never\n        }\n\n        class Document {\n            raw: string\n\n            implements Codec<TextFormat> {\n                type Output = string | int\n\n                function decode(self, input: string) -> string | int {\n                    return self.raw\n                }\n            }\n\n            implements Codec<CodeFormat> {\n                type Output = int | bool\n\n                function decode(self, input: string) -> int | bool {\n                    return 200\n                }\n            }\n        }\n\n        type DecodeOutput =\n            (Document as Codec<TextFormat>).Output | (Document as Codec<CodeFormat>).Output\n\n        function decode_union(doc: Document, text: bool) -> DecodeOutput {\n            if text {\n                return doc.as<Codec<TextFormat, Output = string | int>>.decode(\"\")\n            }\n            return doc.as<Codec<CodeFormat, Output = int | bool>>.decode(\"\")\n        }\n        ",
        },
    )
}

test "match_narrowing_partitions_interface_associated_bindings_in_union" {
    root.compiler.AssertCompiles(
        {
            "match_narrowing_partitions_interface_associated_bindings_in_union.baml": "\n        interface Iterator {\n            type Item\n\n            function next(self) -> Self.Item throws never\n        }\n\n        class IntIterator {\n            implements Iterator {\n                type Item = int\n\n                function next(self) -> int {\n                    return 1\n                }\n            }\n        }\n\n        class StringIterator {\n            implements Iterator {\n                type Item = string\n\n                function next(self) -> string {\n                    return \"s\"\n                }\n            }\n        }\n\n        function label(it: Iterator<Item = int> | Iterator<Item = string>) -> string {\n            return match (it) {\n                let ints: Iterator<Item = int> => \"int\",\n                let strings: Iterator<Item = string> => \"string\",\n            }\n        }\n        ",
        },
    )
}

test "interface_destructure_substitutes_associated_field_type" {
    root.compiler.AssertCompiles(
        {
            "interface_destructure_substitutes_associated_field_type.baml": "\n        interface Source {\n            type Item\n\n            value: Self.Item\n        }\n\n        class IntSource {\n            value: int\n\n            implements Source {\n                type Item = int\n            }\n        }\n\n        function read(source: Source<Item = int>) -> int {\n            return match (source) {\n                Source { value } => value\n            }\n        }\n        ",
        },
    )
}

test "associated_type_union_substitutes_inside_nested_containers" {
    root.compiler.AssertCompiles(
        {
            "associated_type_union_substitutes_inside_nested_containers.baml": "\n        interface Source {\n            type Item\n\n            function list(self) -> Self.Item[] throws never\n            function table(self) -> map<string, Self.Item | null> throws never\n        }\n\n        class MixedSource {\n            values: (int | string)[]\n            table_values: map<string, int | string | null>\n\n            implements Source {\n                type Item = int | string\n\n                function list(self) -> (int | string)[] {\n                    return self.values\n                }\n\n                function table(self) -> map<string, int | string | null> {\n                    return self.table_values\n                }\n            }\n        }\n\n        function list(source: Source<Item = int | string>) -> (int | string)[] {\n            return source.list()\n        }\n\n        function table(source: Source<Item = int | string>) -> map<string, int | string | null> {\n            return source.table()\n        }\n        ",
        },
    )
}

test "concrete_class_associated_projection_accepts_bound_type" {
    root.compiler.AssertCompiles(
        {
            "concrete_class_associated_projection_accepts_bound_type.baml": "\n        interface Carrier {\n            type Item\n\n            function get(self) -> Self.Item throws never\n        }\n\n        class IntCarrier {\n            implements Carrier {\n                type Item = int\n\n                function get(self) -> int {\n                    return 1\n                }\n            }\n        }\n\n        function use_item(carrier: IntCarrier) -> int {\n            let item: IntCarrier.Item = carrier.get()\n            return item\n        }\n        ",
        },
    )
}

test "associated_type_projection_expands_class_type_alias_base" {
    root.compiler.AssertCompiles(
        {
            "associated_type_projection_expands_class_type_alias_base.baml": "\n        interface Iterator {\n            type Item\n\n            function next(self) -> Self.Item? throws never\n        }\n\n        class IntIterator {\n            implements Iterator {\n                type Item = int\n\n                function next(self) -> int? {\n                    return null\n                }\n            }\n        }\n\n        type AliasIterator = IntIterator\n\n        function next(it: IntIterator) -> AliasIterator.Item? {\n            return it.next()\n        }\n        ",
        },
    )
}

test "associated_type_projection_expands_fully_bound_interface_alias_base" {
    root.compiler.AssertCompiles(
        {
            "associated_type_projection_expands_fully_bound_interface_alias_base.baml": "\n        interface Iterator {\n            type Item\n\n            function next(self) -> Self.Item? throws never\n        }\n\n        class IntIterator {\n            implements Iterator {\n                type Item = int\n\n                function next(self) -> int? {\n                    return null\n                }\n            }\n        }\n\n        type IntIteratorInterface = Iterator<Item = int>\n\n        function next(it: IntIteratorInterface) -> IntIteratorInterface.Item? {\n            return it.next()\n        }\n        ",
        },
    )
}

test "associated_types_substitute_inside_nested_type_positions" {
    root.compiler.AssertCompiles(
        {
            "associated_types_substitute_inside_nested_type_positions.baml": "\n        interface Mapper {\n            type Item\n\n            function list(self) -> Self.Item[] throws never\n            function table(self) -> map<string, Self.Item?> throws never\n            function choose(self) -> Self.Item? throws never\n        }\n\n        class IntMapper {\n            items: int[]\n            values: map<string, int?>\n\n            implements Mapper {\n                type Item = int\n\n                function list(self) -> int[] {\n                    return self.items\n                }\n\n                function table(self) -> map<string, int?> {\n                    return self.values\n                }\n\n                function choose(self) -> int? {\n                    return null\n                }\n            }\n        }\n\n        function table(mapper: Mapper<Item = int>) -> map<string, int?> {\n            return mapper.table()\n        }\n        ",
        },
    )
}

test "associated_types_substitute_inside_function_type_positions" {
    root.compiler.AssertCompiles(
        {
            "associated_types_substitute_inside_function_type_positions.baml": "\n        interface Lifter {\n            type Item\n\n            function lift(self) -> ((Self.Item) -> Self.Item? throws never) throws never\n        }\n\n        function lift_int(lifter: Lifter<Item = int>) -> (int) -> int? throws never {\n            return lifter.lift()\n        }\n        ",
        },
    )
}

test "qualified_projection_works_in_nested_types" {
    root.compiler.AssertCompiles(
        {
            "qualified_projection_works_in_nested_types.baml": "\n        class TextFormat {}\n\n        interface Codec<Format> {\n            type Output\n\n            function decode(self, input: string) -> Self.Output throws never\n        }\n\n        class Document {\n            raw: string\n\n            implements Codec<TextFormat> {\n                type Output = string\n\n                function decode(self, input: string) -> string {\n                    return self.raw\n                }\n            }\n        }\n\n        function decode_many(doc: Document) -> (Document as Codec<TextFormat>).Output[] {\n            return [doc.as<Codec<TextFormat, Output = string>>.decode(\"\")]\n        }\n        ",
        },
    )
}

test "qualified_projection_works_in_local_type_annotations" {
    root.compiler.AssertCompiles(
        {
            "qualified_projection_works_in_local_type_annotations.baml": "\n        class TextFormat {}\n\n        interface Codec<Format> {\n            type Output\n\n            function decode(self, input: string) -> Self.Output throws never\n        }\n\n        class Document {\n            raw: string\n\n            implements Codec<TextFormat> {\n                type Output = string\n\n                function decode(self, input: string) -> string {\n                    return self.raw\n                }\n            }\n        }\n\n        function decode(doc: Document) -> string {\n            let output: (Document as Codec<TextFormat>).Output = doc.as<Codec<TextFormat, Output = string>>.decode(\"\")\n            return output\n        }\n        ",
        },
    )
}

test "associated_type_binding_def_typevar_satisfies_declared_bound" {
    root.compiler.AssertCompiles(
        {
            "associated_type_binding_def_typevar_satisfies_declared_bound.baml": "\n        interface Summarizable {\n            function summary(self) -> string throws never\n        }\n\n        interface Holder {\n            type Item extends Summarizable\n            function get(self) -> Self.Item throws never\n        }\n\n        class Box<T> {\n            value: T\n        }\n\n        implements<T extends Summarizable> Holder for Box<T> {\n            type Item = T\n\n            function get(self) -> Self.Item {\n                return self.value\n            }\n        }\n        ",
        },
    )
}

test "associated_type_interface_binding_typevar_satisfies_declared_bound" {
    root.compiler.AssertCompiles(
        {
            "associated_type_interface_binding_typevar_satisfies_declared_bound.baml": "\n        interface Summarizable {\n            function summary(self) -> string throws never\n        }\n\n        interface Holder {\n            type Item extends Summarizable\n            function get(self) -> Self.Item throws never\n        }\n\n        function summarize<T extends Summarizable>(holder: Holder<Item = T>) -> string {\n            return holder.get().summary()\n        }\n        ",
        },
    )
}

test "associated_union_required_method_impl_may_narrow_return_covariantly" {
    root.compiler.AssertCompiles(
        {
            "associated_union_required_method_impl_may_narrow_return_covariantly.baml": "\n        interface Producer {\n            type Item\n\n            function produce(self) -> Self.Item | string throws never\n        }\n\n        class IntProducer {\n            implements Producer {\n                type Item = int\n\n                function produce(self) -> int {\n                    return 1\n                }\n            }\n        }\n        ",
        },
    )
}

test "qualified_projection_from_child_to_bound_parent_compiles" {
    root.compiler.AssertCompiles(
        {
            "qualified_projection_from_child_to_bound_parent_compiles.baml": "\n        interface Parent<T> {\n            type Item = T\n        }\n\n        interface Child requires Parent<int> {}\n\n        type Projected = (Child as Parent<int>).Item\n\n        function use_projected(value: Projected) -> int {\n            return value\n        }\n        ",
        },
    )
}

test "unique_inherited_typevar_projection_unifies_with_parent_slot" {
    root.compiler.AssertCompiles(
        {
            "unique_inherited_typevar_projection_unifies_with_parent_slot.baml": "\n        interface Iterator {\n            type Item\n\n            function next(self) -> Self.Item? throws never\n        }\n\n        interface Sorted requires Iterator {}\n\n        function head<S extends Sorted>(stream: S) -> S.Item? {\n            return stream.next()\n        }\n        ",
        },
    )
}

test "nested_associated_projection_resolves_after_inner_projection" {
    root.compiler.AssertCompiles(
        {
            "nested_associated_projection_resolves_after_inner_projection.baml": "\n        interface InnerCarrier {\n            type Inner\n        }\n\n        class IntInner {\n            implements InnerCarrier {\n                type Inner = int\n            }\n        }\n\n        interface Holder {\n            type Item extends InnerCarrier\n        }\n\n        class IntHolder {\n            implements Holder {\n                type Item = IntInner\n            }\n        }\n\n        type Nested = IntHolder.Item.Inner\n\n        function use_nested(value: Nested) -> int {\n            return value\n        }\n        ",
        },
    )
}

test "associated_interface_alias_projection_compiles" {
    root.compiler.AssertCompiles(
        {
            "associated_interface_alias_projection_compiles.baml": "\n        interface Source {\n            type Item\n\n            function get(self) -> Self.Item throws never\n        }\n\n        type IntSource = Source<Item = int>\n\n        function read(source: IntSource) -> IntSource.Item {\n            return source.get()\n        }\n        ",
        },
    )
}
