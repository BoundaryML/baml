// Reflected compiler diagnostics migrated from the Rust interface suite.
// Every fixture is isolated so intentionally repeated declarations cannot collide.

function ErrorDiagnostics(files: map<string, string>) -> reflect.Diagnostic[] throws never {
    root.compiler.CompileDiagnostics(files).filter((diagnostic) -> bool {
        diagnostic.severity == "error"
    })
}

function AssertHasAnyMessage(files: map<string, string>, needles: string[]) -> void throws never {
    let diagnostics = ErrorDiagnostics(files);
    for (let needle in needles) {
        for (let diagnostic in diagnostics) {
            if (diagnostic.message.includes(needle)) {
                return;
            }
        }
    }
    baml.sys.panic(`expected compiler diagnostic containing one of ${needles}`)
}

test "class_inherent_method_does_not_satisfy_abstract_associated_type_method" {
    root.compiler.AssertHasCode(
        {
            "class_inherent_method_does_not_satisfy_abstract_associated_type_method.baml": `
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
            `,
        },
        "E0113",
    )
}

test "concrete_class_must_match_interface_associated_type_binding" {
    root.compiler.AssertHasCode(
        {
            "concrete_class_must_match_interface_associated_type_binding.baml": `
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
            `,
        },
        "E0001",
    )
}

test "generic_bound_enforces_associated_type_binding" {
    root.compiler.AssertHasCode(
        {
            "generic_bound_enforces_associated_type_binding.baml": `
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
            `,
        },
        "E0001",
    )
}

test "as_upcast_enforces_associated_type_binding" {
    root.compiler.AssertHasCode(
        {
            "as_upcast_enforces_associated_type_binding.baml": `
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
            `,
        },
        "E0001",
    )
}

test "ambiguous_typevar_projection_across_required_interfaces_errors" {
    root.compiler.AssertHasMessage(
        {
            "ambiguous_typevar_projection_across_required_interfaces_errors.baml": `
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
            `,
        },
        "ambiguous associated type projection",
    )
}

test "selected_projection_rejects_mismatched_associated_type_binding" {
    root.compiler.AssertHasCode(
        {
            "selected_projection_rejects_mismatched_associated_type_binding.baml": `
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
            `,
        },
        "E0001",
    )
}

test "qualified_projection_requires_base_to_implement_interface" {
    root.compiler.AssertHasMessage(
        {
            "qualified_projection_requires_base_to_implement_interface.baml": `
                interface Iterator {
                    type Item
                }

                class Box {}

                type Bad = (Box as Iterator).Item
            `,
        },
        "does not implement interface",
    )
}

test "qualified_projection_unknown_associated_member_errors" {
    root.compiler.AssertHasMessage(
        {
            "qualified_projection_unknown_associated_member_errors.baml": `
                interface Iterator {
                    type Item
                }

                class IntIterator {
                    implements Iterator {
                        type Item = int
                    }
                }

                type Bad = (IntIterator as Iterator).Element
            `,
        },
        "unknown associated type `Element`",
    )
}

test "qualified_projection_requires_interface_qualifier" {
    root.compiler.AssertHasMessage(
        {
            "qualified_projection_requires_interface_qualifier.baml": `
                class IntIterator {}

                type Bad = (IntIterator as IntIterator).Item
            `,
        },
        "qualified associated type projection must use an interface",
    )
}

test "qualified_projection_on_unbounded_typevar_requires_interface_bound" {
    root.compiler.AssertHasMessage(
        {
            "qualified_projection_on_unbounded_typevar_requires_interface_bound.baml": `
                interface HasKey {
                    type Key
                    key: Self.Key
                }

                function bad<T>(x: (T as HasKey).Key) -> (T as HasKey).Key {
                    return x
                }
            `,
        },
        "type `T` does not implement interface `HasKey`",
    )
}

test "qualified_projection_on_typevar_rejects_unproven_interface_bound" {
    root.compiler.AssertHasMessage(
        {
            "qualified_projection_on_typevar_rejects_unproven_interface_bound.baml": `
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
            `,
        },
        "type `T` does not implement interface `Other`",
    )
}

test "qualified_projection_on_typevar_rejects_conflicting_associated_binding" {
    root.compiler.AssertHasMessage(
        {
            "qualified_projection_on_typevar_rejects_conflicting_associated_binding.baml": `
                interface HasKey {
                    type Key
                    key: Self.Key
                }

                interface Entity requires HasKey<Key = string> {}

                function bad<T extends Entity>(x: (T as HasKey<Key = int>).Key) -> (T as HasKey<Key = int>).Key {
                    return x
                }
            `,
        },
        "type `T` does not implement interface `HasKey<Key = int>`",
    )
}

test "dependent_associated_type_bound_bare_projection_is_not_an_interface" {
    root.compiler.AssertHasMessage(
        {
            "dependent_associated_type_bound_bare_projection_is_not_an_interface.baml": `
                interface Parser {
                    type Item
                    type Output extends Self.Item

                    function parse(self) -> Self.Output throws never
                }
            `,
        },
        "is not an interface",
    )
}

test "dependent_associated_type_bound_through_interface_rejects_non_implementor" {
    root.compiler.AssertHasMessage(
        {
            "dependent_associated_type_bound_through_interface_rejects_non_implementor.baml": `
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
            `,
        },
        "does not implement bound `Producer<int>`",
    )
}

test "associated_type_bounds_reject_union_types" {
    root.compiler.AssertHasMessage(
        {
            "associated_type_bounds_reject_union_types.baml": `
                interface Parser {
                    type Output extends int | string

                    function parse(self) -> Self.Output throws never
                }
            `,
        },
        "is not an interface",
    )
}

test "selected_union_associated_output_rejects_incompatible_return" {
    root.compiler.AssertHasCode(
        {
            "selected_union_associated_output_rejects_incompatible_return.baml": `
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
            `,
        },
        "E0001",
    )
}

test "unbound_associated_interface_scrutinee_requires_binding" {
    root.compiler.AssertHasMessage(
        {
            "unbound_associated_interface_scrutinee_requires_binding.baml": `
                interface Iterator {
                    type Item

                    function next(self) -> Self.Item throws never
                }

                function label(it: Iterator) -> string {
                    return match (it) {
                        let ints: Iterator<Item = int> => "int",
                    }
                }
            `,
        },
        "must specify its associated type(s) `Item`",
    )
}

test "out_of_body_implements_target_associated_type_binding_errors" {
    root.compiler.AssertHasMessage(
        {
            "out_of_body_implements_target_associated_type_binding_errors.baml": `
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
            `,
        },
        "associated type bindings are not allowed on an `implements` target",
    )
}

test "unbound_interface_existential_requires_pins_even_for_non_associated_methods" {
    root.compiler.AssertHasMessage(
        {
            "unbound_interface_existential_requires_pins_even_for_non_associated_methods.baml": `
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
            `,
        },
        "must specify its associated type",
    )
}

test "unbound_interface_existential_projection_requires_pins" {
    root.compiler.AssertHasMessage(
        {
            "unbound_interface_existential_projection_requires_pins.baml": `
                interface SizedIterator {
                    type Item

                    function next(self) -> Self.Item? throws never
                }

                function next(it: SizedIterator) -> SizedIterator.Item? {
                    return it.next()
                }
            `,
        },
        "must specify its associated type",
    )
}

test "unbound_interface_method_that_returns_associated_type_errors" {
    root.compiler.AssertHasCode(
        {
            "unbound_interface_method_that_returns_associated_type_errors.baml": `
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
            `,
        },
        "E0001",
    )
}

test "concrete_class_associated_projection_rejects_wrong_type" {
    root.compiler.AssertHasCode(
        {
            "concrete_class_associated_projection_rejects_wrong_type.baml": `
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
            `,
        },
        "E0001",
    )
}

test "associated_function_type_projection_rejects_wrong_nested_type" {
    root.compiler.AssertHasCode(
        {
            "associated_function_type_projection_rejects_wrong_nested_type.baml": `
                interface Lifter {
                    type Item

                    function lift(self) -> ((Self.Item) -> Self.Item?) throws never
                }

                function bad(lifter: Lifter<Item = int>) -> (string) -> string? {
                    return lifter.lift()
                }
            `,
        },
        "E0001",
    )
}

test "missing_required_associated_type_binding_errors" {
    root.compiler.AssertHasMessage(
        {
            "missing_required_associated_type_binding_errors.baml": `
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
            `,
        },
        "missing associated type binding",
    )
}

test "duplicate_associated_type_declaration_errors" {
    root.compiler.AssertHasCode(
        {
            "duplicate_associated_type_declaration_errors.baml": `
                interface Bad {
                    type Item
                    type Item = int
                }
            `,
        },
        "E0012",
    )
}

test "associated_type_cannot_collide_with_interface_generic_param" {
    root.compiler.AssertHasMessage(
        {
            "associated_type_cannot_collide_with_interface_generic_param.baml": `
                interface Container<Item> {
                    type Item
                }
            `,
        },
        "collides with generic parameter",
    )
}

test "duplicate_associated_type_binding_errors" {
    root.compiler.AssertHasMessage(
        {
            "duplicate_associated_type_binding_errors.baml": `
                interface Iterator {
                    type Item
                }

                class BadIterator {
                    implements Iterator {
                        type Item = int
                        type Item = string
                    }
                }
            `,
        },
        "is bound more than once",
    )
}

test "implements_target_associated_type_binding_errors" {
    root.compiler.AssertHasMessage(
        {
            "implements_target_associated_type_binding_errors.baml": `
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
            `,
        },
        "associated type bindings are not allowed on an `implements` target",
    )
}

test "implements_target_associated_type_binding_errors_even_when_same_witness_is_in_body" {
    root.compiler.AssertHasMessage(
        {
            "implements_target_associated_type_binding_errors_even_when_same_witness_is_in_body.baml": `
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
            `,
        },
        "associated type bindings are not allowed on an `implements` target",
    )
}

test "impl_associated_type_witness_rejects_extends_bound" {
    root.compiler.AssertHasMessage(
        {
            "impl_associated_type_witness_rejects_extends_bound.baml": `
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
            `,
        },
        "associated type bounds are only allowed on interface declarations",
    )
}

test "implements_target_associated_type_binding_errors_even_with_block_binding" {
    root.compiler.AssertHasMessage(
        {
            "implements_target_associated_type_binding_errors_even_with_block_binding.baml": `
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
            `,
        },
        "associated type bindings are not allowed on an `implements` target",
    )
}

test "duplicate_associated_type_binding_on_interface_value_errors" {
    root.compiler.AssertHasMessage(
        {
            "duplicate_associated_type_binding_on_interface_value_errors.baml": `
                interface Iterator {
                    type Item

                    function next(self) -> Self.Item? throws never
                }

                function bad(it: Iterator<Item = int, Item = string>) -> int {
                    return 0
                }
            `,
        },
        "is bound more than once",
    )
}

test "duplicate_associated_type_binding_on_interface_value_with_union_errors" {
    root.compiler.AssertHasMessage(
        {
            "duplicate_associated_type_binding_on_interface_value_with_union_errors.baml": `
                interface Iterator {
                    type Item

                    function next(self) -> Self.Item throws never
                }

                function bad(it: Iterator<Item = int | string, Item = string>) -> int {
                    return 0
                }
            `,
        },
        "is bound more than once",
    )
}

test "unknown_associated_type_binding_in_implements_errors" {
    root.compiler.AssertHasMessage(
        {
            "unknown_associated_type_binding_in_implements_errors.baml": `
                interface Iterator {
                    type Item
                }

                class BadIterator {
                    implements Iterator {
                        type Element = int
                        type Item = int
                    }
                }
            `,
        },
        "unknown associated type `Element`",
    )
}

test "associated_type_bound_failure_errors" {
    root.compiler.AssertHasCode(
        {
            "associated_type_bound_failure_errors.baml": `
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
            `,
        },
        "E0001",
    )
}

test "associated_type_default_bound_failure_errors" {
    root.compiler.AssertHasCode(
        {
            "associated_type_default_bound_failure_errors.baml": `
                interface Named {
                    name: string
                }

                interface Parser {
                    type Output extends Named = int
                }
            `,
        },
        "E0001",
    )
}

test "abstract_associated_projection_self_param_rejects_unrelated_bound_typevar" {
    root.compiler.AssertHasCode(
        {
            "abstract_associated_projection_self_param_rejects_unrelated_bound_typevar.baml": `
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
            `,
        },
        "E0001",
    )
}

test "associated_union_projection_rejects_narrow_interface_return" {
    root.compiler.AssertHasCode(
        {
            "associated_union_projection_rejects_narrow_interface_return.baml": `
                interface Iterator {
                    type Item

                    function next(self) -> Self.Item throws never
                }

                function bad(it: Iterator<Item = int | string>) -> int {
                    return it.next()
                }
            `,
        },
        "E0001",
    )
}

test "associated_union_projection_rejects_narrow_class_return" {
    root.compiler.AssertHasCode(
        {
            "associated_union_projection_rejects_narrow_class_return.baml": `
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
            `,
        },
        "E0001",
    )
}

test "associated_union_binding_must_satisfy_extends_bound_in_generic_bound" {
    root.compiler.AssertHasMessage(
        {
            "associated_union_binding_must_satisfy_extends_bound_in_generic_bound.baml": `
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
            `,
        },
        "does not implement bound",
    )
}

test "unknown_associated_type_binding_on_interface_value_errors" {
    root.compiler.AssertHasMessage(
        {
            "unknown_associated_type_binding_on_interface_value_errors.baml": `
                interface Iterator {
                    type Item

                    function next(self) -> Self.Item? throws never
                }

                function bad(it: Iterator<Element = int>) -> int {
                    return 0
                }
            `,
        },
        "Element. Did you mean `Item`",
    )
}

test "unknown_associated_type_binding_on_interface_value_with_union_errors" {
    root.compiler.AssertHasMessage(
        {
            "unknown_associated_type_binding_on_interface_value_with_union_errors.baml": `
                interface Iterator {
                    type Item

                    function next(self) -> Self.Item throws never
                }

                function bad(it: Iterator<Element = int | string, Item = int>) -> int {
                    return 0
                }
            `,
        },
        "Element. Did you mean `Item`",
    )
}

test "ambiguous_unqualified_associated_type_projection_errors" {
    root.compiler.AssertHasCode(
        {
            "ambiguous_unqualified_associated_type_projection_errors.baml": `
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
            `,
        },
        "E0001",
    )
}

test "ambiguous_unqualified_projection_across_generic_instantiations_errors" {
    root.compiler.AssertHasCode(
        {
            "ambiguous_unqualified_projection_across_generic_instantiations_errors.baml": `
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
            `,
        },
        "E0001",
    )
}

test "ambiguous_unqualified_projection_type_alias_errors" {
    root.compiler.AssertHasMessage(
        {
            "ambiguous_unqualified_projection_type_alias_errors.baml": `
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
            `,
        },
        "ambiguous associated type `Output`",
    )
}

test "unknown_unqualified_projection_type_alias_errors" {
    root.compiler.AssertHasMessage(
        {
            "unknown_unqualified_projection_type_alias_errors.baml": `
                class Document {}

                type Missing = Document.Output
            `,
        },
        "unknown associated type `Output`",
    )
}

test "bare_default_associated_interface_enforces_default_on_assignment" {
    root.compiler.AssertHasCode(
        {
            "bare_default_associated_interface_enforces_default_on_assignment.baml": `
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
            `,
        },
        "E0001",
    )
}

test "bare_default_associated_generic_bound_enforces_default" {
    root.compiler.AssertHasCode(
        {
            "bare_default_associated_generic_bound_enforces_default.baml": `
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
            `,
        },
        "E0001",
    )
}

test "bare_default_associated_union_member_enforces_default" {
    root.compiler.AssertHasCode(
        {
            "bare_default_associated_union_member_enforces_default.baml": `
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
            `,
        },
        "E0001",
    )
}

test "class_required_parent_must_match_associated_binding" {
    root.compiler.AssertHasMessage(
        {
            "class_required_parent_must_match_associated_binding.baml": `
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
            `,
        },
        "Parent<Item = int>",
    )
}

test "unknown_projection_on_interface_errors" {
    root.compiler.AssertHasMessage(
        {
            "unknown_projection_on_interface_errors.baml": `
                interface Iterator {
                    type Item
                }

                type Missing = Iterator.Element
            `,
        },
        "cannot project `Element` directly off interface `Iterator`",
    )
}

test "unknown_projection_on_interface_alias_errors" {
    root.compiler.AssertHasMessage(
        {
            "unknown_projection_on_interface_alias_errors.baml": `
                interface Iterator {
                    type Item
                }

                type IntIterator = Iterator<Item = int>
                type Missing = IntIterator.Element
            `,
        },
        "cannot project `Element` directly off interface `Iterator`",
    )
}

test "unknown_projection_on_typevar_bound_errors" {
    root.compiler.AssertHasMessage(
        {
            "unknown_projection_on_typevar_bound_errors.baml": `
                interface Iterator {
                    type Item
                }

                function bad<T extends Iterator>(value: T) -> T.Element? {
                    return null
                }
            `,
        },
        "unknown associated type `Element`",
    )
}

test "interface_destructure_head_associated_binding_controls_field_type" {
    root.compiler.AssertHasCode(
        {
            "interface_destructure_head_associated_binding_controls_field_type.baml": `
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
            `,
        },
        "E0001",
    )
}

test "associated_interface_optional_match_requires_null_arm" {
    root.compiler.AssertHasCode(
        {
            "associated_interface_optional_match_requires_null_arm.baml": `
                interface Iterator {
                    type Item

                    function next(self) -> Self.Item? throws never
                }

                function label(it: Iterator<Item = int>?) -> string {
                    return match (it) {
                        let ints: Iterator<Item = int> => "int",
                    }
                }
            `,
        },
        "E0062",
    )
}

test "narrower_associated_interface_pattern_is_rejected" {
    root.compiler.AssertHasMessage(
        {
            "narrower_associated_interface_pattern_is_rejected.baml": `
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
            `,
        },
        "mismatched types: expected `Iterator<Item = int | string>`, found `Iterator<Item = int>`",
    )
}

test "partially_open_associated_binding_overlapping_impls_rejected_across_namespaces" {
    root.compiler.AssertHasCode(
        {
            "ns_contracts/contracts.baml": `
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
            `,
            "ns_models/pair.baml": `
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
            `,
            "ns_app/main.baml": `
                function main() -> string {
                    let p: root.models.Pair<int, string> = root.models.Pair { left: 1, right: "two" }
                    let routed: root.contracts.Routed<Item = string> = p
                    return routed.chosen()
                }
            `,
        },
        "E0132",
    )
}

test "partially_open_associated_binding_overlapping_impls_rejected_structurally" {
    root.compiler.AssertHasCode(
        {
            "partially_open_associated_binding_overlapping_impls_rejected_structurally.baml": `
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
            `,
        },
        "E0132",
    )
}

test "interface_default_method_self_referencing_bound_is_rejected" {
    root.compiler.AssertHasCode(
        {
            "interface_default_method_self_referencing_bound_is_rejected.baml": `
                interface Container {
                    type Item
                    function first(self) -> Self.Item throws never
                    function pick<U extends Self.Item>(self, candidate: U) -> U  throws never {
                        return candidate
                    }
                }
            `,
        },
        "E0145",
    )
}

// BEGIN CUSTOM DIAGNOSTIC PORTS

test "generic_constructor_result_checked_against_interface_associated_binding" {
    let found = false;
    for (
        let diagnostic in ErrorDiagnostics(
            {
                "generic_constructor_result_checked_against_interface_associated_binding.baml": `
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
                `,
            },
        )
    ) {
        if (
            diagnostic.message.includes("Value<Item = string>")
                && diagnostic.message.includes("Box<int>")
        ) {
            found = true;
        }
    }
    assert.equal(found, true)
}

test "own_generic_bound_after_associated_type_still_rejects_at_call" {
    let found = false;
    for (
        let diagnostic in ErrorDiagnostics(
            {
                "own_generic_bound_after_associated_type_still_rejects_at_call.baml": `
                    interface Tagged {
                        function tag_of(self) -> string throws never
                    }
                    interface Tallier {
                        type Marker
                        function tally<X extends Tagged, Y>(self, a: X, extra: Y) -> string throws never {
                            return a.tag_of()
                        }
                    }
                    class Blank {
                        x: int
                    }
                    class Sums {
                        implements Tallier {
                            type Marker = int
                        }
                    }
                    function bad_call(s: Sums, b: Blank) -> string {
                        return s.tally(b, 2)
                    }
                    function main() -> string {
                        return bad_call(Sums {}, Blank { x: 1 })
                    }
                `,
            },
        )
    ) {
        if (
            diagnostic.code == "E0001"
                && diagnostic.message.includes("expected `Tagged`, found `Blank`")
        ) {
            found = true;
        }
    }
    assert.equal(found, true)
}
