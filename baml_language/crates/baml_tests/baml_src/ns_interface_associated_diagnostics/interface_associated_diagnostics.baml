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
            "class_inherent_method_does_not_satisfy_abstract_associated_type_method.baml": "\n        interface Describable {\n            type Output\n\n            function value(self) -> Self.Output throws never\n\n            function describe(self) -> Self.Output throws never {\n                return self.value()\n            }\n        }\n\n        class Ticket {\n            id: string\n\n            implements Describable {\n                type Output = string\n            }\n\n            function value(self) -> Self.Output {\n                return self.id\n            }\n        }\n        ",
        },
        "E0113",
    )
}

test "concrete_class_must_match_interface_associated_type_binding" {
    root.compiler.AssertHasCode(
        {
            "concrete_class_must_match_interface_associated_type_binding.baml": "\n        interface Iterator {\n            type Item\n\n            function next(self) -> Self.Item? throws never\n        }\n\n        class IntIterator {\n            implements Iterator {\n                type Item = int\n\n                function next(self) -> int? {\n                    return null\n                }\n            }\n        }\n\n        function bad() -> string? {\n            let it: Iterator<Item = string> = IntIterator {}\n            return it.next()\n        }\n        ",
        },
        "E0001",
    )
}

test "generic_bound_enforces_associated_type_binding" {
    root.compiler.AssertHasCode(
        {
            "generic_bound_enforces_associated_type_binding.baml": "\n        interface Iterator {\n            type Item\n\n            function next(self) -> Self.Item? throws never\n        }\n\n        class IntIterator {\n            implements Iterator {\n                type Item = int\n\n                function next(self) -> int? {\n                    return null\n                }\n            }\n        }\n\n        function take_string<I extends Iterator<Item = string>>(it: I) -> string? {\n            return it.next()\n        }\n\n        function bad(it: IntIterator) -> string? {\n            return take_string<IntIterator>(it)\n        }\n        ",
        },
        "E0001",
    )
}

test "as_upcast_enforces_associated_type_binding" {
    root.compiler.AssertHasCode(
        {
            "as_upcast_enforces_associated_type_binding.baml": "\n        interface Iterator {\n            type Item\n\n            function next(self) -> Self.Item? throws never\n        }\n\n        class IntIterator {\n            implements Iterator {\n                type Item = int\n\n                function next(self) -> int? {\n                    return null\n                }\n            }\n        }\n\n        function bad(it: IntIterator) -> string? {\n            return it.as<Iterator<Item = string>>.next()\n        }\n        ",
        },
        "E0001",
    )
}

test "ambiguous_typevar_projection_across_required_interfaces_errors" {
    root.compiler.AssertHasMessage(
        {
            "ambiguous_typevar_projection_across_required_interfaces_errors.baml": "\n        interface Iterator {\n            type Item\n\n            function next(self) -> Self.Item? throws never\n        }\n\n        interface Reader {\n            type Item\n\n            function read(self) -> Self.Item throws never\n        }\n\n        interface Stream requires Iterator, Reader {}\n\n        function bad<S extends Stream>(stream: S) -> S.Item? {\n            return stream.next()\n        }\n        ",
        },
        "ambiguous associated type projection",
    )
}

test "selected_projection_rejects_mismatched_associated_type_binding" {
    root.compiler.AssertHasCode(
        {
            "selected_projection_rejects_mismatched_associated_type_binding.baml": "\n        class TextFormat {}\n\n        interface Codec<Format> {\n            type Output\n\n            function decode(self, input: string) -> Self.Output throws never\n        }\n\n        class Document {\n            raw: string\n\n            implements Codec<TextFormat> {\n                type Output=string\n\n                function decode(self, input: string) -> string {\n                    return self.raw\n                }\n            }\n        }\n\n        function bad(doc: Document) -> (Document as Codec<TextFormat, Output = int>).Output {\n            return doc.as<Codec<TextFormat, Output = string>>.decode(\"\")\n        }\n        ",
        },
        "E0001",
    )
}

test "qualified_projection_requires_base_to_implement_interface" {
    root.compiler.AssertHasMessage(
        {
            "qualified_projection_requires_base_to_implement_interface.baml": "\n        interface Iterator {\n            type Item\n        }\n\n        class Box {}\n\n        type Bad = (Box as Iterator).Item\n        ",
        },
        "does not implement interface",
    )
}

test "qualified_projection_unknown_associated_member_errors" {
    root.compiler.AssertHasMessage(
        {
            "qualified_projection_unknown_associated_member_errors.baml": "\n        interface Iterator {\n            type Item\n        }\n\n        class IntIterator {\n            implements Iterator {\n                type Item = int\n            }\n        }\n\n        type Bad = (IntIterator as Iterator).Element\n        ",
        },
        "unknown associated type `Element`",
    )
}

test "qualified_projection_requires_interface_qualifier" {
    root.compiler.AssertHasMessage(
        {
            "qualified_projection_requires_interface_qualifier.baml": "\n        class IntIterator {}\n\n        type Bad = (IntIterator as IntIterator).Item\n        ",
        },
        "qualified associated type projection must use an interface",
    )
}

test "qualified_projection_on_unbounded_typevar_requires_interface_bound" {
    root.compiler.AssertHasMessage(
        {
            "qualified_projection_on_unbounded_typevar_requires_interface_bound.baml": "\n        interface HasKey {\n            type Key\n            key: Self.Key\n        }\n\n        function bad<T>(x: (T as HasKey).Key) -> (T as HasKey).Key {\n            return x\n        }\n        ",
        },
        "type `T` does not implement interface `HasKey`",
    )
}

test "qualified_projection_on_typevar_rejects_unproven_interface_bound" {
    root.compiler.AssertHasMessage(
        {
            "qualified_projection_on_typevar_rejects_unproven_interface_bound.baml": "\n        interface HasKey {\n            type Key\n            key: Self.Key\n        }\n\n        interface Entity requires HasKey<Key = string> {}\n\n        interface Other {\n            type Key\n            key: Self.Key\n        }\n\n        function bad<T extends Entity>(x: (T as Other).Key) -> (T as Other).Key {\n            return x\n        }\n        ",
        },
        "type `T` does not implement interface `Other`",
    )
}

test "qualified_projection_on_typevar_rejects_conflicting_associated_binding" {
    root.compiler.AssertHasMessage(
        {
            "qualified_projection_on_typevar_rejects_conflicting_associated_binding.baml": "\n        interface HasKey {\n            type Key\n            key: Self.Key\n        }\n\n        interface Entity requires HasKey<Key = string> {}\n\n        function bad<T extends Entity>(x: (T as HasKey<Key = int>).Key) -> (T as HasKey<Key = int>).Key {\n            return x\n        }\n        ",
        },
        "type `T` does not implement interface `HasKey<Key = int>`",
    )
}

test "dependent_associated_type_bound_bare_projection_is_not_an_interface" {
    root.compiler.AssertHasMessage(
        {
            "dependent_associated_type_bound_bare_projection_is_not_an_interface.baml": "\n        interface Parser {\n            type Item\n            type Output extends Self.Item\n\n            function parse(self) -> Self.Output throws never\n        }\n        ",
        },
        "is not an interface",
    )
}

test "dependent_associated_type_bound_through_interface_rejects_non_implementor" {
    root.compiler.AssertHasMessage(
        {
            "dependent_associated_type_bound_through_interface_rejects_non_implementor.baml": "\n        interface Producer<T> {\n            function make(self) -> T throws never\n        }\n\n        interface Parser {\n            type Item\n            type Output extends Producer<Self.Item>\n        }\n\n        class IntParser {\n            implements Parser {\n                type Item = int\n                type Output = string\n            }\n        }\n        ",
        },
        "does not implement bound `Producer<int>`",
    )
}

test "associated_type_bounds_reject_union_types" {
    root.compiler.AssertHasMessage(
        {
            "associated_type_bounds_reject_union_types.baml": "\n        interface Parser {\n            type Output extends int | string\n\n            function parse(self) -> Self.Output throws never\n        }\n        ",
        },
        "is not an interface",
    )
}

test "selected_union_associated_output_rejects_incompatible_return" {
    root.compiler.AssertHasCode(
        {
            "selected_union_associated_output_rejects_incompatible_return.baml": "\n        class TextFormat {}\n\n        interface Codec<Format> {\n            type Output\n\n            function decode(self, input: string) -> Self.Output throws never\n        }\n\n        class Document {\n            raw: string\n\n            implements Codec<TextFormat> {\n                type Output = string | int\n\n                function decode(self, input: string) -> string | int {\n                    return self.raw\n                }\n            }\n        }\n\n        function bad(doc: Document) -> bool {\n            return doc.as<Codec<TextFormat, Output = string | int>>.decode(\"\")\n        }\n        ",
        },
        "E0001",
    )
}

test "unbound_associated_interface_scrutinee_requires_binding" {
    root.compiler.AssertHasMessage(
        {
            "unbound_associated_interface_scrutinee_requires_binding.baml": "\n        interface Iterator {\n            type Item\n\n            function next(self) -> Self.Item throws never\n        }\n\n        function label(it: Iterator) -> string {\n            return match (it) {\n                let ints: Iterator<Item = int> => \"int\",\n            }\n        }\n        ",
        },
        "must specify its associated type(s) `Item`",
    )
}

test "out_of_body_implements_target_associated_type_binding_errors" {
    root.compiler.AssertHasMessage(
        {
            "out_of_body_implements_target_associated_type_binding_errors.baml": "\n        interface Iterator {\n            type Item\n\n            function next(self) -> Self.Item? throws never\n        }\n\n        class IntIterator {}\n\n        implements Iterator<Item = int> for IntIterator {\n            type Item = int\n\n            function next(self) -> int? {\n                return null\n            }\n        }\n        ",
        },
        "associated type bindings are not allowed on an `implements` target",
    )
}

test "unbound_interface_existential_requires_pins_even_for_non_associated_methods" {
    root.compiler.AssertHasMessage(
        {
            "unbound_interface_existential_requires_pins_even_for_non_associated_methods.baml": "\n        interface SizedIterator {\n            type Item\n\n            function size(self) -> int throws never\n            function next(self) -> Self.Item? throws never\n        }\n\n        class IntIterator {\n            implements SizedIterator {\n                type Item = int\n\n                function size(self) -> int {\n                    return 1\n                }\n\n                function next(self) -> int? {\n                    return null\n                }\n            }\n        }\n\n        function count(it: SizedIterator) -> int {\n            return it.size()\n        }\n        ",
        },
        "must specify its associated type",
    )
}

test "unbound_interface_existential_projection_requires_pins" {
    root.compiler.AssertHasMessage(
        {
            "unbound_interface_existential_projection_requires_pins.baml": "\n        interface SizedIterator {\n            type Item\n\n            function next(self) -> Self.Item? throws never\n        }\n\n        function next(it: SizedIterator) -> SizedIterator.Item? {\n            return it.next()\n        }\n        ",
        },
        "must specify its associated type",
    )
}

test "unbound_interface_method_that_returns_associated_type_errors" {
    root.compiler.AssertHasCode(
        {
            "unbound_interface_method_that_returns_associated_type_errors.baml": "\n        interface Iterator {\n            type Item\n\n            function next(self) -> Self.Item? throws never\n        }\n\n        class IntIterator {\n            implements Iterator {\n                type Item = int\n\n                function next(self) -> int? {\n                    return null\n                }\n            }\n        }\n\n        function bad(it: Iterator) -> int? {\n            return it.next()\n        }\n        ",
        },
        "E0001",
    )
}

test "concrete_class_associated_projection_rejects_wrong_type" {
    root.compiler.AssertHasCode(
        {
            "concrete_class_associated_projection_rejects_wrong_type.baml": "\n        interface Carrier {\n            type Item\n\n            function get(self) -> Self.Item throws never\n        }\n\n        class IntCarrier {\n            implements Carrier {\n                type Item = int\n\n                function get(self) -> int {\n                    return 1\n                }\n            }\n        }\n\n        function bad(carrier: IntCarrier) -> string {\n            return carrier.get()\n        }\n        ",
        },
        "E0001",
    )
}

test "associated_function_type_projection_rejects_wrong_nested_type" {
    root.compiler.AssertHasCode(
        {
            "associated_function_type_projection_rejects_wrong_nested_type.baml": "\n        interface Lifter {\n            type Item\n\n            function lift(self) -> ((Self.Item) -> Self.Item?) throws never\n        }\n\n        function bad(lifter: Lifter<Item = int>) -> (string) -> string? {\n            return lifter.lift()\n        }\n        ",
        },
        "E0001",
    )
}

test "missing_required_associated_type_binding_errors" {
    root.compiler.AssertHasMessage(
        {
            "missing_required_associated_type_binding_errors.baml": "\n        interface Iterator {\n            type Item\n\n            function next(self) -> Self.Item? throws never\n        }\n\n        class BadIterator {\n            implements Iterator {\n                function next(self) -> int? {\n                    return null\n                }\n            }\n        }\n        ",
        },
        "missing associated type binding",
    )
}

test "duplicate_associated_type_declaration_errors" {
    root.compiler.AssertHasCode(
        {
            "duplicate_associated_type_declaration_errors.baml": "\n        interface Bad {\n            type Item\n            type Item = int\n        }\n        ",
        },
        "E0012",
    )
}

test "associated_type_cannot_collide_with_interface_generic_param" {
    root.compiler.AssertHasMessage(
        {
            "associated_type_cannot_collide_with_interface_generic_param.baml": "\n        interface Container<Item> {\n            type Item\n        }\n        ",
        },
        "collides with generic parameter",
    )
}

test "duplicate_associated_type_binding_errors" {
    root.compiler.AssertHasMessage(
        {
            "duplicate_associated_type_binding_errors.baml": "\n        interface Iterator {\n            type Item\n        }\n\n        class BadIterator {\n            implements Iterator {\n                type Item = int\n                type Item = string\n            }\n        }\n        ",
        },
        "is bound more than once",
    )
}

test "implements_target_associated_type_binding_errors" {
    root.compiler.AssertHasMessage(
        {
            "implements_target_associated_type_binding_errors.baml": "\n        interface Iterator {\n            type Item\n\n            function next(self) -> Self.Item? throws never\n        }\n\n        class IntIterator {\n            implements Iterator<Item = int> {\n                function next(self) -> int? {\n                    return null\n                }\n            }\n        }\n        ",
        },
        "associated type bindings are not allowed on an `implements` target",
    )
}

test "implements_target_associated_type_binding_errors_even_when_same_witness_is_in_body" {
    root.compiler.AssertHasMessage(
        {
            "implements_target_associated_type_binding_errors_even_when_same_witness_is_in_body.baml": "\n        interface Iterator {\n            type Item\n\n            function next(self) -> Self.Item? throws never\n        }\n\n        class IntIterator {\n            implements Iterator<Item = int> {\n                type Item = int\n\n                function next(self) -> int? {\n                    return null\n                }\n            }\n        }\n        ",
        },
        "associated type bindings are not allowed on an `implements` target",
    )
}

test "impl_associated_type_witness_rejects_extends_bound" {
    root.compiler.AssertHasMessage(
        {
            "impl_associated_type_witness_rejects_extends_bound.baml": "\n        interface Iterator {\n            type Item\n\n            function next(self) -> Self.Item? throws never\n        }\n\n        class IntIterator {\n            implements Iterator {\n                type Item extends int = int\n\n                function next(self) -> int? {\n                    return null\n                }\n            }\n        }\n        ",
        },
        "associated type bounds are only allowed on interface declarations",
    )
}

test "implements_target_associated_type_binding_errors_even_with_block_binding" {
    root.compiler.AssertHasMessage(
        {
            "implements_target_associated_type_binding_errors_even_with_block_binding.baml": "\n        interface Iterator {\n            type Item\n\n            function next(self) -> Self.Item? throws never\n        }\n\n        class BadIterator {\n            implements Iterator<Item = string> {\n                type Item = int\n\n                function next(self) -> int? {\n                    return null\n                }\n            }\n        }\n        ",
        },
        "associated type bindings are not allowed on an `implements` target",
    )
}

test "duplicate_associated_type_binding_on_interface_value_errors" {
    root.compiler.AssertHasMessage(
        {
            "duplicate_associated_type_binding_on_interface_value_errors.baml": "\n        interface Iterator {\n            type Item\n\n            function next(self) -> Self.Item? throws never\n        }\n\n        function bad(it: Iterator<Item = int, Item = string>) -> int {\n            return 0\n        }\n        ",
        },
        "is bound more than once",
    )
}

test "duplicate_associated_type_binding_on_interface_value_with_union_errors" {
    root.compiler.AssertHasMessage(
        {
            "duplicate_associated_type_binding_on_interface_value_with_union_errors.baml": "\n        interface Iterator {\n            type Item\n\n            function next(self) -> Self.Item throws never\n        }\n\n        function bad(it: Iterator<Item = int | string, Item = string>) -> int {\n            return 0\n        }\n        ",
        },
        "is bound more than once",
    )
}

test "unknown_associated_type_binding_in_implements_errors" {
    root.compiler.AssertHasMessage(
        {
            "unknown_associated_type_binding_in_implements_errors.baml": "\n        interface Iterator {\n            type Item\n        }\n\n        class BadIterator {\n            implements Iterator {\n                type Element = int\n                type Item = int\n            }\n        }\n        ",
        },
        "unknown associated type `Element`",
    )
}

test "associated_type_bound_failure_errors" {
    root.compiler.AssertHasCode(
        {
            "associated_type_bound_failure_errors.baml": "\n        interface Named {\n            name: string\n        }\n\n        interface Parser {\n            type Output extends Named\n\n            function parse(self) -> Self.Output throws never\n        }\n\n        class BadParser {\n            implements Parser {\n                type Output = int\n\n                function parse(self) -> int {\n                    return 1\n                }\n            }\n        }\n        ",
        },
        "E0001",
    )
}

test "associated_type_default_bound_failure_errors" {
    root.compiler.AssertHasCode(
        {
            "associated_type_default_bound_failure_errors.baml": "\n        interface Named {\n            name: string\n        }\n\n        interface Parser {\n            type Output extends Named = int\n        }\n        ",
        },
        "E0001",
    )
}

test "abstract_associated_projection_self_param_rejects_unrelated_bound_typevar" {
    root.compiler.AssertHasCode(
        {
            "abstract_associated_projection_self_param_rejects_unrelated_bound_typevar.baml": "\n        interface Comparable {\n            function same(self, other: Self) -> bool throws never\n        }\n\n        interface Holder {\n            type Item extends Comparable\n\n            function left(self) -> Self.Item throws never\n        }\n\n        function compare<H extends Holder, C extends Comparable>(holder: H, other: C) -> bool {\n            return holder.left().same(other)\n        }\n        ",
        },
        "E0001",
    )
}

test "associated_union_projection_rejects_narrow_interface_return" {
    root.compiler.AssertHasCode(
        {
            "associated_union_projection_rejects_narrow_interface_return.baml": "\n        interface Iterator {\n            type Item\n\n            function next(self) -> Self.Item throws never\n        }\n\n        function bad(it: Iterator<Item = int | string>) -> int {\n            return it.next()\n        }\n        ",
        },
        "E0001",
    )
}

test "associated_union_projection_rejects_narrow_class_return" {
    root.compiler.AssertHasCode(
        {
            "associated_union_projection_rejects_narrow_class_return.baml": "\n        interface Iterator {\n            type Item\n\n            function next(self) -> Self.Item throws never\n        }\n\n        class MixedIterator {\n            value: int | string\n\n            implements Iterator {\n                type Item = int | string\n\n                function next(self) -> Self.Item {\n                    return self.value\n                }\n            }\n        }\n\n        function bad(it: MixedIterator) -> int {\n            return it.next()\n        }\n        ",
        },
        "E0001",
    )
}

test "associated_union_binding_must_satisfy_extends_bound_in_generic_bound" {
    root.compiler.AssertHasMessage(
        {
            "associated_union_binding_must_satisfy_extends_bound_in_generic_bound.baml": "\n        interface Named {\n            name: string\n        }\n\n        class Label {\n            name: string\n            implements Named {}\n        }\n\n        interface Parser {\n            type Output extends Named\n\n            function parse(self) -> Self.Output throws never\n        }\n\n        function bad<P extends Parser<Output = Label | int>>(parser: P) -> P.Output {\n            return parser.parse()\n        }\n        ",
        },
        "does not implement bound",
    )
}

test "unknown_associated_type_binding_on_interface_value_errors" {
    root.compiler.AssertHasMessage(
        {
            "unknown_associated_type_binding_on_interface_value_errors.baml": "\n        interface Iterator {\n            type Item\n\n            function next(self) -> Self.Item? throws never\n        }\n\n        function bad(it: Iterator<Element = int>) -> int {\n            return 0\n        }\n        ",
        },
        "Element. Did you mean `Item`",
    )
}

test "unknown_associated_type_binding_on_interface_value_with_union_errors" {
    root.compiler.AssertHasMessage(
        {
            "unknown_associated_type_binding_on_interface_value_with_union_errors.baml": "\n        interface Iterator {\n            type Item\n\n            function next(self) -> Self.Item throws never\n        }\n\n        function bad(it: Iterator<Element = int | string, Item = int>) -> int {\n            return 0\n        }\n        ",
        },
        "Element. Did you mean `Item`",
    )
}

test "ambiguous_unqualified_associated_type_projection_errors" {
    root.compiler.AssertHasCode(
        {
            "ambiguous_unqualified_associated_type_projection_errors.baml": "\n        interface Iterator {\n            type Item\n\n            function next(self) -> Self.Item? throws never\n        }\n\n        interface Reader {\n            type Item\n\n            function read(self) -> Self.Item throws never\n        }\n\n        class File {\n            implements Iterator {\n                type Item = string\n\n                function next(self) -> string? {\n                    return null\n                }\n            }\n\n            implements Reader {\n                type Item = int\n\n                function read(self) -> int {\n                    return 1\n                }\n            }\n        }\n\n        function bad(file: File) -> File.Item? {\n            return file.next()\n        }\n        ",
        },
        "E0001",
    )
}

test "ambiguous_unqualified_projection_across_generic_instantiations_errors" {
    root.compiler.AssertHasCode(
        {
            "ambiguous_unqualified_projection_across_generic_instantiations_errors.baml": "\n        class TextFormat {}\n        class CodeFormat {}\n\n        interface Codec<Format> {\n            type Output\n\n            function decode(self, input: string) -> Self.Output throws never\n        }\n\n        class Document {\n            raw: string\n\n            implements Codec<TextFormat> {\n                type Output = string\n\n                function decode(self, input: string) -> string {\n                    return self.raw\n                }\n            }\n\n            implements Codec<CodeFormat> {\n                type Output = int\n\n                function decode(self, input: string) -> int {\n                    return 200\n                }\n            }\n        }\n\n        function bad(doc: Document) -> Document.Output {\n            return doc.as<Codec<TextFormat, Output = string>>.decode(\"\")\n        }\n        ",
        },
        "E0001",
    )
}

test "ambiguous_unqualified_projection_type_alias_errors" {
    root.compiler.AssertHasMessage(
        {
            "ambiguous_unqualified_projection_type_alias_errors.baml": "\n        class TextFormat {}\n        class CodeFormat {}\n\n        interface Codec<Format> {\n            type Output\n        }\n\n        class Document {\n            implements Codec<TextFormat> {\n                type Output = string\n            }\n\n            implements Codec<CodeFormat> {\n                type Output = int\n            }\n        }\n\n        type Ambiguous = Document.Output\n        ",
        },
        "ambiguous associated type `Output`",
    )
}

test "unknown_unqualified_projection_type_alias_errors" {
    root.compiler.AssertHasMessage(
        {
            "unknown_unqualified_projection_type_alias_errors.baml": "\n        class Document {}\n\n        type Missing = Document.Output\n        ",
        },
        "unknown associated type `Output`",
    )
}

test "bare_default_associated_interface_enforces_default_on_assignment" {
    root.compiler.AssertHasCode(
        {
            "bare_default_associated_interface_enforces_default_on_assignment.baml": "\n        interface Decoder {\n            type Output = string\n\n            function decode(self, input: string) -> Self.Output throws never\n        }\n\n        class StatusDecoder {\n            implements Decoder {\n                type Output = int\n\n                function decode(self, input: string) -> int {\n                    return 200\n                }\n            }\n        }\n\n        function bad() -> string {\n            let decoder: Decoder = StatusDecoder {}\n            return decoder.decode(\"\")\n        }\n        ",
        },
        "E0001",
    )
}

test "bare_default_associated_generic_bound_enforces_default" {
    root.compiler.AssertHasCode(
        {
            "bare_default_associated_generic_bound_enforces_default.baml": "\n        interface Decoder {\n            type Output = string\n\n            function decode(self, input: string) -> Self.Output throws never\n        }\n\n        class StatusDecoder {\n            implements Decoder {\n                type Output = int\n\n                function decode(self, input: string) -> int {\n                    return 200\n                }\n            }\n        }\n\n        function take_default<D extends Decoder>(decoder: D) -> string {\n            return decoder.decode(\"\")\n        }\n\n        function bad(decoder: StatusDecoder) -> string {\n            return take_default<StatusDecoder>(decoder)\n        }\n        ",
        },
        "E0001",
    )
}

test "bare_default_associated_union_member_enforces_default" {
    root.compiler.AssertHasCode(
        {
            "bare_default_associated_union_member_enforces_default.baml": "\n        interface Decoder {\n            type Output = string\n\n            function decode(self, input: string) -> Self.Output throws never\n        }\n\n        class StatusDecoder {\n            implements Decoder {\n                type Output = int\n\n                function decode(self, input: string) -> int {\n                    return 200\n                }\n            }\n        }\n\n        function bad() -> Decoder | null {\n            return StatusDecoder {}\n        }\n        ",
        },
        "E0001",
    )
}

test "class_required_parent_must_match_associated_binding" {
    root.compiler.AssertHasMessage(
        {
            "class_required_parent_must_match_associated_binding.baml": "\n        interface Parent {\n            type Item\n\n            function get(self) -> Self.Item throws never\n        }\n\n        interface Child requires Parent<Item = int> {}\n\n        class BadChild {\n            implements Parent {\n                type Item = string\n\n                function get(self) -> string {\n                    return \"bad\"\n                }\n            }\n\n            implements Child {}\n        }\n        ",
        },
        "Parent<Item = int>",
    )
}

test "unknown_projection_on_interface_errors" {
    root.compiler.AssertHasMessage(
        {
            "unknown_projection_on_interface_errors.baml": "\n        interface Iterator {\n            type Item\n        }\n\n        type Missing = Iterator.Element\n        ",
        },
        "cannot project `Element` directly off interface `Iterator`",
    )
}

test "unknown_projection_on_interface_alias_errors" {
    root.compiler.AssertHasMessage(
        {
            "unknown_projection_on_interface_alias_errors.baml": "\n        interface Iterator {\n            type Item\n        }\n\n        type IntIterator = Iterator<Item = int>\n        type Missing = IntIterator.Element\n        ",
        },
        "cannot project `Element` directly off interface `Iterator`",
    )
}

test "unknown_projection_on_typevar_bound_errors" {
    root.compiler.AssertHasMessage(
        {
            "unknown_projection_on_typevar_bound_errors.baml": "\n        interface Iterator {\n            type Item\n        }\n\n        function bad<T extends Iterator>(value: T) -> T.Element? {\n            return null\n        }\n        ",
        },
        "unknown associated type `Element`",
    )
}

test "interface_destructure_head_associated_binding_controls_field_type" {
    root.compiler.AssertHasCode(
        {
            "interface_destructure_head_associated_binding_controls_field_type.baml": "\n        interface Source {\n            type Item\n\n            value: Self.Item\n        }\n\n        function read(source: Source<Item = int> | Source<Item = string>) -> int {\n            return match (source) {\n                Source<Item = string> { value } => value,\n                _ => 0,\n            }\n        }\n        ",
        },
        "E0001",
    )
}

test "associated_interface_optional_match_requires_null_arm" {
    root.compiler.AssertHasCode(
        {
            "associated_interface_optional_match_requires_null_arm.baml": "\n        interface Iterator {\n            type Item\n\n            function next(self) -> Self.Item? throws never\n        }\n\n        function label(it: Iterator<Item = int>?) -> string {\n            return match (it) {\n                let ints: Iterator<Item = int> => \"int\",\n            }\n        }\n        ",
        },
        "E0062",
    )
}

test "narrower_associated_interface_pattern_is_rejected" {
    root.compiler.AssertHasMessage(
        {
            "narrower_associated_interface_pattern_is_rejected.baml": "\n        interface Iterator {\n            type Item\n\n            function next(self) -> Self.Item throws never\n        }\n\n        function label(it: Iterator<Item = int | string>) -> string {\n            return match (it) {\n                let ints: Iterator<Item = int> => \"int\",\n                let strings: Iterator<Item = string> => \"string\",\n            }\n        }\n        ",
        },
        "mismatched types: expected `Iterator<Item = int | string>`, found `Iterator<Item = int>`",
    )
}

test "partially_open_associated_binding_overlapping_impls_rejected_across_namespaces" {
    root.compiler.AssertHasCode(
        {
            "ns_contracts/contracts.baml": "\n                interface Bucket<T> {\n                    type Shape\n\n                    function get(self) -> string throws never\n                }\n\n                interface Routed requires Bucket<Self.Item, Shape = map<Self.Item, int>> {\n                    type Item\n\n                    function chosen(self) -> string throws never {\n                        return self.get()\n                    }\n                }\n                ",
            "ns_models/pair.baml": "\n                class Pair<L, R> {\n                    left: L\n                    right: R\n\n                    implements root.contracts.Bucket<L> {\n                        type Shape = map<L, int>\n\n                        function get(self) -> string {\n                            return \"left\"\n                        }\n                    }\n\n                    implements root.contracts.Bucket<R> {\n                        type Shape = map<R, int>\n\n                        function get(self) -> string {\n                            return \"right\"\n                        }\n                    }\n\n                    implements root.contracts.Routed {\n                        type Item = R\n                    }\n                }\n                ",
            "ns_app/main.baml": "\n                function main() -> string {\n                    let p: root.models.Pair<int, string> = root.models.Pair { left: 1, right: \"two\" }\n                    let routed: root.contracts.Routed<Item = string> = p\n                    return routed.chosen()\n                }\n                ",
        },
        "E0132",
    )
}

test "partially_open_associated_binding_overlapping_impls_rejected_structurally" {
    root.compiler.AssertHasCode(
        {
            "partially_open_associated_binding_overlapping_impls_rejected_structurally.baml": "\n        interface Bucket<T> {\n            type Shape\n\n            function get(self) -> string throws never\n        }\n\n        interface Routed requires Bucket<Self.Item, Shape = map<Self.Item, int>> {\n            type Item\n\n            function chosen(self) -> string throws never {\n                return self.get()\n            }\n        }\n\n        class Pair<L, R> {\n            left: L\n            right: R\n\n            implements Bucket<L> {\n                type Shape = map<L, int>\n\n                function get(self) -> string {\n                    return \"left\"\n                }\n            }\n\n            implements Bucket<R> {\n                type Shape = map<R, int>\n\n                function get(self) -> string {\n                    return \"right\"\n                }\n            }\n\n            implements Routed {\n                type Item = R\n            }\n        }\n\n        function main() -> string {\n            let p: Pair<int, string> = Pair { left: 1, right: \"two\" }\n            let routed: Routed<Item = string> = p\n            return routed.chosen()\n        }\n        ",
        },
        "E0132",
    )
}

test "interface_default_method_self_referencing_bound_is_rejected" {
    root.compiler.AssertHasCode(
        {
            "interface_default_method_self_referencing_bound_is_rejected.baml": "\n        interface Container {\n            type Item\n            function first(self) -> Self.Item throws never\n            function pick<U extends Self.Item>(self, candidate: U) -> U  throws never {\n                return candidate\n            }\n        }\n        ",
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
                "generic_constructor_result_checked_against_interface_associated_binding.baml": "\n        interface Value {\n            type Item\n\n            function get(self) -> Self.Item throws never\n        }\n\n        class Box<T> {\n            value: T\n\n            function new(value: T) -> Box<T> {\n                return Box<T> { value: value }\n            }\n\n            implements Value {\n                type Item = T\n\n                function get(self) -> T {\n                    return self.value\n                }\n            }\n        }\n\n        function bad() -> string {\n            let n: int = 1\n            let value: Value<Item = string> = Box.new(n)\n            return value.get()\n        }\n        ",
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
                "own_generic_bound_after_associated_type_still_rejects_at_call.baml": "\n        interface Tagged {\n            function tag_of(self) -> string throws never\n        }\n        interface Tallier {\n            type Marker\n            function tally<X extends Tagged, Y>(self, a: X, extra: Y) -> string throws never {\n                return a.tag_of()\n            }\n        }\n        class Blank {\n            x: int\n        }\n        class Sums {\n            implements Tallier {\n                type Marker = int\n            }\n        }\n        function bad_call(s: Sums, b: Blank) -> string {\n            return s.tally(b, 2)\n        }\n        function main() -> string {\n            return bad_call(Sums {}, Blank { x: 1 })\n        }\n        ",
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
