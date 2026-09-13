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

function AssertHasDiagnostic(
    files: map<string, string>,
    code: string,
    needles: string[],
) -> void throws never {
    for (let diagnostic in ErrorDiagnostics(files)) {
        if (code == "" || diagnostic.code == code) {
            let matches = true;
            for (let needle in needles) {
                if (!diagnostic.message.includes(needle)) {
                    matches = false;
                }
            }
            if (matches) {
                return;
            }
        }
    }
    baml.sys.panic(`expected diagnostic ${code} containing ${needles}`)
}

function AssertLacksCode(files: map<string, string>, code: string) -> void throws never {
    for (let diagnostic in ErrorDiagnostics(files)) {
        if (diagnostic.code == code) {
            baml.sys.panic(`unexpected compiler diagnostic code ${code}`)
        }
    }
}

function AssertLacksDiagnostic(files: map<string, string>, needles: string[]) -> void throws never {
    for (let diagnostic in ErrorDiagnostics(files)) {
        let matches = true;
        for (let needle in needles) {
            if (!diagnostic.message.includes(needle)) {
                matches = false;
            }
        }
        if (matches) {
            baml.sys.panic(`unexpected compiler diagnostic containing ${needles}`)
        }
    }
}

function AssertCodeCount(
    files: map<string, string>,
    code: string,
    expected: int,
) -> void throws never {
    let count = 0;
    for (let diagnostic in ErrorDiagnostics(files)) {
        if (diagnostic.code == code) {
            count += 1;
        }
    }
    assert.equal(count, expected)
}

function AssertErrorCount(files: map<string, string>, expected: int) -> void throws never {
    assert.equal(ErrorDiagnostics(files).length(), expected)
}

function AssertHasErrors(files: map<string, string>) -> void throws never {
    if (ErrorDiagnostics(files).length() == 0) {
        baml.sys.panic("expected at least one compiler error")
    }
}

function AssertAllErrorsContain(files: map<string, string>, needle: string) -> void throws never {
    for (let diagnostic in ErrorDiagnostics(files)) {
        if (!diagnostic.message.includes(needle)) {
            baml.sys.panic(`unexpected diagnostic without ${needle}: ${diagnostic.message}`)
        }
    }
}

function AssertAllErrorsLack(files: map<string, string>, needles: string[]) -> void throws never {
    for (let diagnostic in ErrorDiagnostics(files)) {
        for (let needle in needles) {
            if (diagnostic.message.includes(needle)) {
                baml.sys.panic(`unexpected diagnostic containing ${needle}: ${diagnostic.message}`)
            }
        }
    }
}

test "missing_required_method_is_compile_error" {
    root.compiler.AssertHasCode(
        {
            "missing_required_method_is_compile_error.baml": "\n        interface Animal {\n            function speak(self) -> string throws never\n        }\n\n        class Incomplete {\n            implements Animal {}\n        }\n        ",
        },
        "E0113",
    )
}

test "missing_required_method_message_names_method_and_interface" {
    root.compiler.AssertHasMessage(
        {
            "missing_required_method_message_names_method_and_interface.baml": "\n        interface Animal {\n            function speak(self) -> string throws never\n        }\n        class Mute {\n            implements Animal {}\n        }\n        ",
        },
        "method `speak` required by interface `Animal`",
    )
}

test "implements_unknown_interface_is_compile_error" {
    root.compiler.AssertHasCode(
        {
            "implements_unknown_interface_is_compile_error.baml": "\n        class Ghost {\n            implements DoesNotExist {}\n        }\n        ",
        },
        "E0002",
    )
}

test "implementing_a_class_is_not_allowed" {
    root.compiler.AssertHasCode(
        {
            "implementing_a_class_is_not_allowed.baml": "\n        class Foo { x: int }\n        class Bar {\n            implements Foo {}\n        }\n        ",
        },
        "E0119",
    )
}

test "implementing_an_enum_is_not_allowed" {
    root.compiler.AssertHasCode(
        {
            "implementing_an_enum_is_not_allowed.baml": "\n        enum Color { Red, Green, Blue }\n        class Bar {\n            implements Color {}\n        }\n        ",
        },
        "E0119",
    )
}

test "orphan_rule_rejects_blanket_impl_of_foreign_interface" {
    root.compiler.AssertHasCode(
        {
            "orphan_rule_rejects_blanket_impl_of_foreign_interface.baml": "\n        implement<T> baml.ops.Equals for T {}\n        ",
        },
        "E0139",
    )
}

test "orphan_rule_rejects_foreign_interface_for_foreign_type" {
    root.compiler.AssertHasCode(
        {
            "orphan_rule_rejects_foreign_interface_for_foreign_type.baml": "\n        implement baml.ops.Equals for int {}\n        ",
        },
        "E0139",
    )
}

test "unknown_method_in_implements_block_is_compile_error" {
    root.compiler.AssertHasCode(
        {
            "unknown_method_in_implements_block_is_compile_error.baml": "\n        interface Animal {\n            function speak(self) -> string throws never\n        }\n        class Dog {\n            implements Animal {\n                function speak(self) -> string { return \"woof\" }\n                function bark(self) -> string { return \"bark\" }\n            }\n        }\n        ",
        },
        "E0115",
    )
}

test "duplicate_implements_block_is_compile_error" {
    root.compiler.AssertHasCode(
        {
            "duplicate_implements_block_is_compile_error.baml": "\n        interface Animal {\n            function speak(self) -> string throws never\n        }\n\n        class Dog {\n            implements Animal {\n                function speak(self) -> string { return \"a\" }\n            }\n            implements Animal {\n                function speak(self) -> string { return \"b\" }\n            }\n        }\n        ",
        },
        "E0132",
    )
}

test "duplicate_same_generic_interface_instantiation_is_compile_error" {
    root.compiler.AssertHasCode(
        {
            "duplicate_same_generic_interface_instantiation_is_compile_error.baml": "\n        interface Converter<T> {\n            function convert(self) -> T throws never\n        }\n\n        class MultiFormat {\n            implements Converter<int> {\n                function convert(self) -> int { return 1 }\n            }\n            implements Converter<int> {\n                function convert(self) -> int { return 2 }\n            }\n        }\n        ",
        },
        "E0132",
    )
}

test "duplicate_generic_interface_instantiation_via_alias_is_compile_error" {
    root.compiler.AssertHasCode(
        {
            "duplicate_generic_interface_instantiation_via_alias_is_compile_error.baml": "\n        interface Converter<T> {\n            function convert(self) -> T throws never\n        }\n\n        type IntAlias = int\n\n        class MultiFormat {\n            implements Converter<IntAlias> {\n                function convert(self) -> int { return 1 }\n            }\n            implements Converter<int> {\n                function convert(self) -> int { return 2 }\n            }\n        }\n        ",
        },
        "E0132",
    )
}

test "nested_alias_duplicate_implements_is_detected" {
    root.compiler.AssertHasCode(
        {
            "nested_alias_duplicate_implements_is_detected.baml": "\n        interface Converter<T> {\n            function convert(self) -> T throws never\n        }\n\n        type IntList = int[]\n\n        class MultiFormat {\n            implements Converter<IntList[]> {\n                function convert(self) -> int[][] { return [[1]] }\n            }\n            implements Converter<int[][]> {\n                function convert(self) -> int[][] { return [[2]] }\n            }\n        }\n        ",
        },
        "E0132",
    )
}

test "requires_cycle_is_compile_error" {
    root.compiler.AssertHasCode(
        {
            "requires_cycle_is_compile_error.baml": "\n        interface A requires B {}\n        interface B requires A {}\n        ",
        },
        "E0118",
    )
}

test "three_way_requires_cycle_is_compile_error" {
    root.compiler.AssertHasCode(
        {
            "three_way_requires_cycle_is_compile_error.baml": "\n        interface A requires B {}\n        interface B requires C {}\n        interface C requires A {}\n        ",
        },
        "E0118",
    )
}

test "missing_class_field_for_interface_field_is_error" {
    root.compiler.AssertHasCode(
        {
            "missing_class_field_for_interface_field_is_error.baml": "\n        interface Config {\n            host: string\n            port: int\n        }\n\n        class Server {\n            host: string\n            implements Config {}\n        }\n        ",
        },
        "E0124",
    )
}

test "interface_extends_clause_is_rejected" {
    root.compiler.AssertHasMessage(
        {
            "interface_extends_clause_is_rejected.baml": "\n        interface Named {\n            name: string\n        }\n        interface Person extends Named {\n            age: int\n        }\n        ",
        },
        "extends",
    )
}

test "implementing_requires_chain_satisfies_parent_required_methods" {
    root.compiler.AssertHasMessage(
        {
            "implementing_requires_chain_satisfies_parent_required_methods.baml": "\n        interface Named { name: string }\n        interface Aged { age: int }\n        interface Person requires Named, Aged {\n            occupation: string\n            function introduce(self) -> string throws never\n        }\n\n        class Employee {\n            salary: float\n            implements Person {}\n        }\n        ",
        },
        "method `introduce` required by interface `Person`",
    )
}

test "requires_chain_required_method_must_be_provided" {
    root.compiler.AssertHasMessage(
        {
            "requires_chain_required_method_must_be_provided.baml": "\n        interface Greeter {\n            function greet(self) -> string throws never\n        }\n        interface Person requires Greeter {\n            name: string\n        }\n\n        class Bob {\n            implements Person {}\n        }\n        ",
        },
        "also requires implementing `Greeter`",
    )
}

test "class_without_implements_cannot_satisfy_interface_param" {
    root.compiler.AssertHasMessage(
        {
            "class_without_implements_cannot_satisfy_interface_param.baml": "\n        interface Animal {\n            function speak(self) -> string throws never\n        }\n        // Note: no `implements Animal`.\n        class Robot {\n            function speak(self) -> string { return \"beep\" }\n        }\n\n        function describe(a: Animal) -> string {\n            return \"an animal\"\n        }\n\n        function main() -> string {\n            let r = Robot {}\n            return describe(r)\n        }\n        ",
        },
        "Robot",
    )
}

test "method_signature_mismatch_return_type" {
    root.compiler.AssertHasCode(
        {
            "method_signature_mismatch_return_type.baml": "\n        interface Animal {\n            function speak(self) -> string throws never\n        }\n        class Robot {\n            implements Animal {\n                function speak(self) -> int { return 42 }\n            }\n        }\n        ",
        },
        "E0120",
    )
}

test "method_signature_mismatch_param_type" {
    root.compiler.AssertHasCode(
        {
            "method_signature_mismatch_param_type.baml": "\n        interface Adder {\n            function add(self, a: int, b: int) -> int throws never\n        }\n        class BadAdder {\n            implements Adder {\n                function add(self, a: string, b: string) -> int { return 0 }\n            }\n        }\n        ",
        },
        "E0120",
    )
}

test "class_own_method_does_not_resolve_ambiguous_interface_method_call" {
    root.compiler.AssertHasCode(
        {
            "class_own_method_does_not_resolve_ambiguous_interface_method_call.baml": "\n        interface Serializer {\n            function encode(self) -> string throws never\n        }\n        interface BinarySerializer {\n            function encode(self) -> string throws never\n        }\n        class Hybrid {\n            function encode(self) -> string { return \"class\" }\n            implements Serializer {\n                function encode(self) -> string { return \"json\" }\n            }\n            implements BinarySerializer {\n                function encode(self) -> string { return \"bin\" }\n            }\n        }\n        function main() -> string {\n            let h = Hybrid {}\n            return h.encode()\n        }\n        ",
        },
        "E0121",
    )
}

test "qualified_interface_field_construction_is_compile_error" {
    root.compiler.AssertHasMessage(
        {
            "qualified_interface_field_construction_is_compile_error.baml": "\n        interface Config {\n            host: string\n        }\n        class Server {\n            host: string\n            implements Config {}\n        }\n        function main() -> string {\n            let s = Server { Config.host: \"localhost\" }\n            return s.host\n        }\n        ",
        },
        "Config.host",
    )
}

test "simple_interface_field_view_construction_is_compile_error" {
    root.compiler.AssertHasMessage(
        {
            "simple_interface_field_view_construction_is_compile_error.baml": "\n        interface Named {\n            name: string\n        }\n        class Person {\n            title: string\n            implements Named {\n                name as title\n            }\n        }\n        function main() -> string {\n            let p = Person { name: \"Ada\" }\n            return p.title\n        }\n        ",
        },
        "use class field `title`",
    )
}

test "interface_field_type_must_match_invariantly" {
    root.compiler.AssertHasCode(
        {
            "interface_field_type_must_match_invariantly.baml": "\n        interface Measured {\n            value: int | string\n        }\n        class Count {\n            value: int\n            implements Measured {}\n        }\n        ",
        },
        "E0116",
    )
}

test "unqualified_same_name_interface_field_access_is_ambiguous" {
    root.compiler.AssertHasMessage(
        {
            "unqualified_same_name_interface_field_access_is_ambiguous.baml": "\n        interface Named { name: string }\n        interface Labeled { name: string }\n        class Item {\n            named_name: string\n            labeled_name: string\n            implements Named { name as named_name }\n            implements Labeled { name as labeled_name }\n        }\n        function main() -> string {\n            let i = Item { named_name: \"widget\", labeled_name: \"WIDGET-001\" }\n            return i.name\n        }\n        ",
        },
        "as<Named>",
    )
}

test "generic_interface_field_links_swapped_type_var_impls_overlap" {
    root.compiler.AssertHasCode(
        {
            "generic_interface_field_links_swapped_type_var_impls_overlap.baml": "\n        interface Slot<T, E> {\n            value: T\n        }\n        class Pair<L, R> {\n            left: L\n            right: R\n            implements Slot<L, R> {\n                value as left\n            }\n            implements Slot<R, L> {\n                value as right\n            }\n        }\n        function main() -> bool {\n            let p: Pair<int, string> = Pair { left: 7, right: \"seven\" }\n            let lr: Slot<int, string> = p\n            let rl: Slot<string, int> = p\n            return lr.value == 7 && rl.value == \"seven\"\n        }\n    ",
        },
        "E0132",
    )
}

test "generic_interface_method_swapped_type_var_impls_overlap" {
    root.compiler.AssertHasCode(
        {
            "generic_interface_method_swapped_type_var_impls_overlap.baml": "\n        interface Reporter<T, E> {\n            function show(self) -> T throws never\n        }\n        class Pair<L, R> {\n            left: L\n            right: R\n            implements Reporter<L, R> {\n                function show(self) -> L {\n                    return self.left\n                }\n            }\n            implements Reporter<R, L> {\n                function show(self) -> R {\n                    return self.right\n                }\n            }\n        }\n        function main() -> bool {\n            let p: Pair<int, string> = Pair { left: 7, right: \"seven\" }\n            let lr: Reporter<int, string> = p\n            let rl: Reporter<string, int> = p\n            return lr.show() == 7 && rl.show() == \"seven\"\n        }\n    ",
        },
        "E0132",
    )
}

test "old_interface_qualified_projection_is_compile_error" {
    root.compiler.AssertHasMessage(
        {
            "old_interface_qualified_projection_is_compile_error.baml": "\n        interface Animal {\n            function speak(self) -> string throws never\n        }\n        class Dog {\n            implements Animal {\n                function speak(self) -> string { return \"Woof!\" }\n            }\n        }\n        function bad() -> string {\n            let d = Dog {}\n            return d.Animal.speak()\n        }\n        ",
        },
        "has no member `Animal`",
    )
}

test "self_param_method_rejects_heterogeneous_generic_args" {
    root.compiler.AssertHasMessage(
        {
            "self_param_method_rejects_heterogeneous_generic_args.baml": "\n        interface Equatable {\n            function eq(self, other: Self) -> bool throws never\n        }\n        function cmp<S extends Equatable, U extends Equatable>(x: S, y: U) -> bool {\n            return x.eq(y)\n        }\n        ",
        },
        "expected `S`, found `U`",
    )
}

test "self_param_method_rejects_mismatched_literal_arg" {
    root.compiler.AssertHasMessage(
        {
            "self_param_method_rejects_mismatched_literal_arg.baml": "\n        interface Equatable {\n            function eq(self, other: Self) -> bool throws never\n        }\n        function bad<T extends Equatable>(x: T) -> bool {\n            return x.eq(5)\n        }\n        ",
        },
        "expected `T`, found `5`",
    )
}

test "self_param_method_rejects_nested_self_mismatch" {
    root.compiler.AssertHasMessage(
        {
            "self_param_method_rejects_nested_self_mismatch.baml": "\n        interface Adder {\n            function addAll(self, others: Self[]) -> int throws never\n        }\n        function cross<T extends Adder, U extends Adder>(x: T, ys: U[]) -> int {\n            return x.addAll(ys)\n        }\n        ",
        },
        "expected `T[]`, found `U[]`",
    )
}

test "bound_self_method_value_rejects_heterogeneous_arg" {
    root.compiler.AssertHasMessage(
        {
            "bound_self_method_value_rejects_heterogeneous_arg.baml": "\n        interface Adder {\n            function addOne(self, other: Self) -> int throws never\n        }\n        function cross<T extends Adder, U extends Adder>(x: T, y: U) -> int {\n            let f = x.addOne\n            return f(y)\n        }\n        ",
        },
        "expected `T`, found `U`",
    )
}

test "mixed_interface_generic_overlapping_type_arg_impls_overlap" {
    root.compiler.AssertHasCode(
        {
            "mixed_interface_generic_overlapping_type_arg_impls_overlap.baml": "\n        interface Slot<T> {\n            value: T\n        }\n        class Pair<L, R> {\n            left: L\n            right: R\n            implements Slot<L> {\n                value as left\n            }\n            implements Slot<R> {\n                value as right\n            }\n        }\n\n        function describe(s: Slot<string>) -> string {\n            return match (s) {\n                Pair<int, string> { left: 7 } => \"pair\"\n                Slot<string> { value: string } => \"slot\"\n            }\n        }\n        ",
        },
        "E0132",
    )
}

test "generic_interface_and_concrete_destructures_still_report_partial_coverage" {
    root.compiler.AssertHasCode(
        {
            "generic_interface_and_concrete_destructures_still_report_partial_coverage.baml": "\n        interface Slot<T> {\n            value: T\n        }\n        class Pair<L, R> {\n            left: L\n            right: R\n            implements Slot<L> {\n                value as left\n            }\n        }\n\n        function describe(s: Slot<int>) -> string {\n            return match (s) {\n                Pair<int, string> { right: \"seven\" } => \"pair\"\n                Slot<int> { value: 1 } => \"one\"\n            }\n        }\n        ",
        },
        "E0062",
    )
}

test "union_of_generic_interfaces_reports_missing_interface_branch" {
    root.compiler.AssertHasCode(
        {
            "union_of_generic_interfaces_reports_missing_interface_branch.baml": "\n        interface Slot<T> {\n            value: T\n        }\n        class Pair<L, R> {\n            left: L\n            right: R\n            implements Slot<L> {\n                value as left\n            }\n        }\n\n        interface Cargo<T> {\n            payload: T\n        }\n        class Box<T> {\n            inner: T\n            label: string\n            implements Cargo<T> {\n                payload as inner\n            }\n        }\n\n        function describe(x: Slot<int> | Cargo<string>) -> string {\n            return match (x) {\n                Pair<int, string> { right: \"tag\" } => \"pair\"\n                Slot<int> { value: int } => \"slot\"\n            }\n        }\n        ",
        },
        "E0062",
    )
}

test "mixed_interface_and_concrete_destructures_still_report_partial_coverage" {
    root.compiler.AssertHasCode(
        {
            "mixed_interface_and_concrete_destructures_still_report_partial_coverage.baml": "\n        interface Animal {\n            name: string\n            friendly: bool\n        }\n        class Dog {\n            name: string\n            friendly: bool\n            implements Animal {}\n        }\n\n        function describe(a: Animal) -> string {\n            return match (a) {\n                Dog { friendly: true } => \"friendly\"\n                Animal { name: \"Rex\" } => \"rex\"\n            }\n        }\n        ",
        },
        "E0062",
    )
}

test "class_inherent_method_does_not_satisfy_interface_method" {
    root.compiler.AssertHasCode(
        {
            "class_inherent_method_does_not_satisfy_interface_method.baml": "\n        interface Greeter {\n            function greet(self) -> string throws never\n        }\n\n        class Person {\n            implements Greeter {}\n\n            function greet(self) -> int {\n                return 1\n            }\n        }\n        ",
        },
        "E0113",
    )
}

test "generic_bound_violation_is_compile_error" {
    root.compiler.AssertHasMessage(
        {
            "generic_bound_violation_is_compile_error.baml": "\n        interface Named { name: string }\n        function first_name<T extends Named>(items: T[]) -> string {\n            return items[0].name\n        }\n        function main() -> string {\n            let xs: int[] = [1, 2, 3]\n            return first_name<int>(xs)\n        }\n        ",
        },
        "Named",
    )
}

test "generic_bound_violation_in_instantiation_expr_is_compile_error" {
    root.compiler.AssertHasMessage(
        {
            "generic_bound_violation_in_instantiation_expr_is_compile_error.baml": "\n        interface Named { name: string }\n        function first_name<T extends Named>(items: T[]) -> string {\n            return items[0].name\n        }\n        function main() -> string {\n            let f = first_name<int>;\n            return \"ok\"\n        }\n        ",
        },
        "Named",
    )
}

test "generic_bound_alias_syntax_is_compile_error" {
    root.compiler.AssertHasMessage(
        {
            "generic_bound_alias_syntax_is_compile_error.baml": "\n        interface Converter<T> {\n            function convert(self) -> T throws never\n        }\n        function read_int<T extends Converter<int> as Ints>(m: T) -> int {\n            return m.as<Converter<int>>.convert()\n        }\n        ",
        },
        "generic bound aliases are not supported",
    )
}

test "same_interface_different_type_args_is_not_assignable" {
    root.compiler.AssertHasMessage(
        {
            "same_interface_different_type_args_is_not_assignable.baml": "\n        interface Box<T> {\n            function get(self) -> T throws never\n        }\n        function bad(x: Box<int>) -> Box<string> {\n            return x\n        }\n        ",
        },
        "mismatched types",
    )
}

test "generic_interface_method_explicit_type_args_are_checked" {
    root.compiler.AssertHasMessage(
        {
            "generic_interface_method_explicit_type_args_are_checked.baml": "\n        interface Echo<T> {\n            function echo<U>(self, value: U) -> U throws never\n        }\n        class Echoer {\n            implements Echo<int> {\n                function echo<U>(self, value: U) -> U {\n                    return value\n                }\n            }\n        }\n        function bad() -> string {\n            let e: Echo<int> = Echoer {}\n            return e.echo<int>(\"nope\")\n        }\n        ",
        },
        "mismatched types",
    )
}

test "class_type_reference_rejects_wrong_generic_arg_count" {
    root.compiler.AssertHasMessage(
        {
            "class_type_reference_rejects_wrong_generic_arg_count.baml": "\n        class Box<T> {\n            value: T\n        }\n        function bad(value: Box<int, string>) -> int {\n            return 1\n        }\n        ",
        },
        "expects 1 type argument(s), got 2",
    )
}

test "class_type_reference_rejects_too_few_explicit_generic_arg_count" {
    root.compiler.AssertHasMessage(
        {
            "class_type_reference_rejects_too_few_explicit_generic_arg_count.baml": "\n        class Pair<L, R> {\n            left: L\n            right: R\n        }\n        function bad(value: Pair<int>) -> int {\n            return 1\n        }\n        ",
        },
        "expects 2 type argument(s), got 1",
    )
}

test "function_call_rejects_wrong_explicit_generic_arg_count" {
    root.compiler.AssertHasMessage(
        {
            "function_call_rejects_wrong_explicit_generic_arg_count.baml": "\n        function id<T>(value: T) -> T {\n            return value\n        }\n        function bad() -> int {\n            return id<int, string>(1)\n        }\n        ",
        },
        "function `id` expects 1 type argument(s), got 2",
    )
}

test "function_call_rejects_too_few_explicit_generic_arg_count" {
    root.compiler.AssertHasMessage(
        {
            "function_call_rejects_too_few_explicit_generic_arg_count.baml": "\n        function pair<L, R>(left: L, right: R) -> L {\n            return left\n        }\n        function bad() -> int {\n            return pair<int>(1, \"nope\")\n        }\n        ",
        },
        "function `pair` expects 2 type argument(s), got 1",
    )
}

test "in_body_implements_rejects_wrong_interface_generic_arg_count" {
    root.compiler.AssertHasMessage(
        {
            "in_body_implements_rejects_wrong_interface_generic_arg_count.baml": "\n        interface Label<T> {\n            function label(self) -> T throws never\n        }\n        class Thing {\n            implements Label<int, string> {\n                function label(self) -> int {\n                    return 1\n                }\n            }\n        }\n        ",
        },
        "expects 1 type argument(s), got 2",
    )
}

test "in_body_implements_rejects_too_few_explicit_interface_generic_arg_count" {
    root.compiler.AssertHasMessage(
        {
            "in_body_implements_rejects_too_few_explicit_interface_generic_arg_count.baml": "\n        interface PairLabel<L, R> {\n            function label(self) -> L throws never\n        }\n        class Thing {\n            implements PairLabel<int> {\n                function label(self) -> int {\n                    return 1\n                }\n            }\n        }\n        ",
        },
        "expects 2 type argument(s), got 1",
    )
}

test "out_of_body_implements_rejects_wrong_interface_generic_arg_count" {
    root.compiler.AssertHasMessage(
        {
            "out_of_body_implements_rejects_wrong_interface_generic_arg_count.baml": "\n        interface Label<T> {\n            function label(self) -> T throws never\n        }\n        class Thing {}\n        implements Label<int, string> for Thing {\n            function label(self) -> int {\n                return 1\n            }\n        }\n        ",
        },
        "expects 1 type argument(s), got 2",
    )
}

test "out_of_body_implements_rejects_too_few_explicit_interface_generic_arg_count" {
    root.compiler.AssertHasMessage(
        {
            "out_of_body_implements_rejects_too_few_explicit_interface_generic_arg_count.baml": "\n        interface PairLabel<L, R> {\n            function label(self) -> L throws never\n        }\n        class Thing {}\n        implements PairLabel<int> for Thing {\n            function label(self) -> int {\n                return 1\n            }\n        }\n        ",
        },
        "expects 2 type argument(s), got 1",
    )
}

test "out_of_body_implements_rejects_wrong_target_class_generic_arg_count" {
    root.compiler.AssertHasMessage(
        {
            "out_of_body_implements_rejects_wrong_target_class_generic_arg_count.baml": "\n        interface Marker {}\n        class Box<T> {\n            value: T\n        }\n        implements Marker for Box<int, string> {}\n        ",
        },
        "expects 1 type argument(s), got 2",
    )
}

test "out_of_body_implements_rejects_too_few_explicit_target_class_generic_arg_count" {
    root.compiler.AssertHasMessage(
        {
            "out_of_body_implements_rejects_too_few_explicit_target_class_generic_arg_count.baml": "\n        interface Marker {}\n        class Pair<L, R> {\n            left: L\n            right: R\n        }\n        implements Marker for Pair<int> {}\n        ",
        },
        "expects 2 type argument(s), got 1",
    )
}

test "required_interface_method_generic_bound_mismatch_is_error" {
    root.compiler.AssertHasCode(
        {
            "required_interface_method_generic_bound_mismatch_is_error.baml": "\n        interface Named { name: string }\n        interface Reader {\n            function read<T>(self, value: T) -> string throws never\n        }\n        class ReaderImpl {\n            implements Reader {\n                function read<T extends Named>(self, value: T) -> string {\n                    return value.name\n                }\n            }\n        }\n        ",
        },
        "E0120",
    )
}

test "generic_bounds_reject_compound_type_expressions" {
    root.compiler.AssertHasMessage(
        {
            "generic_bounds_reject_compound_type_expressions.baml": "\n        function keep_union<T extends int | string>(x: T) -> int {\n            return 1\n        }\n        function main() -> int {\n            return 1\n        }\n        ",
        },
        "is not an interface",
    )
}

test "multi_self_method_rejected_on_interface_typed_receiver" {
    root.compiler.AssertHasMessage(
        {
            "multi_self_method_rejected_on_interface_typed_receiver.baml": "\n        interface Equatable {\n            function same(self, other: Self) -> bool throws never\n        }\n        class Box {\n            value: int\n            implements Equatable {\n                function same(self, other: Self) -> bool {\n                    return self.value == other.value\n                }\n            }\n        }\n        function bad(a: Equatable, b: Equatable) -> bool {\n            return a.same(b)\n        }\n        ",
        },
        "concrete receiver",
    )
}

test "concrete_receiver_self_param_method_rejects_wrong_arg" {
    root.compiler.AssertHasMessage(
        {
            "concrete_receiver_self_param_method_rejects_wrong_arg.baml": "\n        interface Equals {\n            function eq(self, other: Self) -> bool throws never\n            function neq(self, other: Self) -> bool throws never {\n                return !self.eq(other)\n            }\n        }\n        class Num {\n            v: int\n            implements Equals {\n                function eq(self, other: Self) -> bool { return self.v == other.v }\n            }\n        }\n        class Other { w: int }\n        function main() -> bool {\n            return Num { v: 1 }.neq(Other { w: 2 })\n        }\n        ",
        },
        "expected `Num`, found `Other`",
    )
}

test "unbounded_generic_forwarded_to_bounded_call_is_rejected" {
    root.compiler.AssertHasMessage(
        {
            "unbounded_generic_forwarded_to_bounded_call_is_rejected.baml": "\n        interface Equatable {\n            function eq(self, other: Self) -> bool throws never\n        }\n        function same<T extends Equatable>(x: T) -> bool {\n            return x.eq(x)\n        }\n        function forward<U>(x: U) -> bool {\n            return same(x)\n        }\n        ",
        },
        "expected `Equatable`, found `U`",
    )
}

test "unbounded_generic_forwarded_through_container_is_rejected" {
    root.compiler.AssertHasMessage(
        {
            "unbounded_generic_forwarded_through_container_is_rejected.baml": "\n        interface Equatable {\n            function eq(self, other: Self) -> bool throws never\n        }\n        function firstEq<T extends Equatable>(xs: T[]) -> bool {\n            return xs[0].eq(xs[0])\n        }\n        function forward<U>(xs: U[]) -> bool {\n            return firstEq(xs)\n        }\n        ",
        },
        "expected `Equatable`, found `U`",
    )
}

test "self_type_outside_interface_or_class_is_compile_error" {
    root.compiler.AssertHasMessage(
        {
            "self_type_outside_interface_or_class_is_compile_error.baml": "\n        function bad(x: Self) -> Self {\n            return x\n        }\n        ",
        },
        "Self",
    )
}

test "cast_from_one_interface_to_another_when_class_implements_both_is_error" {
    root.compiler.AssertHasMessage(
        {
            "cast_from_one_interface_to_another_when_class_implements_both_is_error.baml": "\n        interface Animal {\n            function speak(self) -> string throws never\n        }\n        interface Swimmer {\n            function swim(self) -> string throws never\n        }\n        class Dog {\n            implements Animal {\n                function speak(self) -> string { return \"Woof!\" }\n            }\n            implements Swimmer {\n                function swim(self) -> string { return \"splash\" }\n            }\n        }\n        function main() -> string {\n            let a: Animal = Dog {}\n            let s: Swimmer = a\n            return s.swim()\n        }\n    ",
        },
        "Animal",
    )
}

test "as_rejects_interface_downcast" {
    root.compiler.AssertHasMessage(
        {
            "as_rejects_interface_downcast.baml": "\n        interface Animal {\n            function speak(self) -> string throws never\n        }\n        interface Swimmer {\n            function swim(self) -> string throws never\n        }\n        class Dog {\n            implements Animal {\n                function speak(self) -> string { return \"Woof!\" }\n            }\n            implements Swimmer {\n                function swim(self) -> string { return \"splash\" }\n            }\n        }\n        function bad(a: Animal) -> string {\n            let s = a.as<Swimmer>\n            return s.swim()\n        }\n        ",
        },
        "Animal",
    )
}

test "as_requires_interface_target" {
    root.compiler.AssertHasMessage(
        {
            "as_requires_interface_target.baml": "\n        interface Animal {\n            function speak(self) -> string throws never\n        }\n        class Dog {\n            implements Animal {\n                function speak(self) -> string { return \"Woof!\" }\n            }\n        }\n        function bad(d: Dog) -> Dog {\n            return d.as<Dog>\n        }\n        ",
        },
        "expected an interface qualifier",
    )
}

test "default_keyword_outside_implements_block_is_compile_error" {
    root.compiler.AssertHasMessage(
        {
            "default_keyword_outside_implements_block_is_compile_error.baml": "\n        interface Logger {\n            function log(self, msg: string) -> string throws never { return msg }\n        }\n        function rogue(msg: string) -> string {\n            return default.log(msg)\n        }\n        ",
        },
        "default",
    )
}

test "default_keyword_inside_class_method_outside_implements_is_compile_error" {
    root.compiler.AssertHasMessage(
        {
            "default_keyword_inside_class_method_outside_implements_is_compile_error.baml": "\n        interface Logger {\n            function log(self, msg: string) -> string throws never { return msg }\n        }\n        class Helper {\n            function rogue(self, msg: string) -> string {\n                return default.log(msg)\n            }\n        }\n        ",
        },
        "default",
    )
}

test "default_keyword_inside_lambda_inside_implements_block" {
    root.compiler.AssertHasMessage(
        {
            "default_keyword_inside_lambda_inside_implements_block.baml": "\n        interface Logger {\n            function log(self, msg: string) -> string throws never { return msg }\n        }\n        class L {\n            implements Logger {\n                function log(self, msg: string) -> string {\n                    let f = (m: string) => default.log(m)\n                    return f(msg)\n                }\n            }\n        }\n        ",
        },
        "default",
    )
}

test "missing_class_field_for_interface_with_partial_links_is_error" {
    root.compiler.AssertHasCode(
        {
            "missing_class_field_for_interface_with_partial_links_is_error.baml": "\n        interface Config {\n            host: string\n            port: int\n        }\n        class Server {\n            host: string\n            implements Config {\n                host as host\n            }\n        }\n        ",
        },
        "E0124",
    )
}

test "declaring_field_in_implements_block_is_error" {
    root.compiler.AssertHasCode(
        {
            "declaring_field_in_implements_block_is_error.baml": "\n        interface Config {\n            port: int\n        }\n        class Server {\n            implements Config {\n                port: string\n            }\n        }\n        ",
        },
        "E0127",
    )
}

test "unknown_interface_field_in_link_is_error" {
    root.compiler.AssertHasCode(
        {
            "unknown_interface_field_in_link_is_error.baml": "\n        interface Named {\n            name: string\n        }\n        class Person {\n            name: string\n            implements Named {\n                title as name\n            }\n        }\n        ",
        },
        "E0128",
    )
}

test "unknown_class_field_in_link_is_error" {
    root.compiler.AssertHasCode(
        {
            "unknown_class_field_in_link_is_error.baml": "\n        interface Named {\n            name: string\n        }\n        class Person {\n            name: string\n            implements Named {\n                name as display_name\n            }\n        }\n        ",
        },
        "E0129",
    )
}

test "duplicate_interface_field_link_is_error" {
    root.compiler.AssertHasCode(
        {
            "duplicate_interface_field_link_is_error.baml": "\n        interface Named {\n            name: string\n        }\n        class Person {\n            primary_name: string\n            secondary_name: string\n            implements Named {\n                name as primary_name\n                name as secondary_name\n            }\n        }\n        ",
        },
        "E0130",
    )
}

test "missing_required_parent_interface_is_error" {
    root.compiler.AssertHasCode(
        {
            "missing_required_parent_interface_is_error.baml": "\n        interface Named { name: string }\n        interface Aged { age: int }\n        interface Person requires Named, Aged {\n            occupation: string\n        }\n        class Bad {\n            name: string\n            age: int\n            occupation: string\n            implements Person {}\n        }\n        ",
        },
        "E0125",
    )
}

test "missing_one_of_two_required_parents_is_error" {
    root.compiler.AssertHasCode(
        {
            "missing_one_of_two_required_parents_is_error.baml": "\n        interface Named { name: string }\n        interface Aged { age: int }\n        interface Person requires Named, Aged {\n            occupation: string\n        }\n        class Partial {\n            name: string\n            age: int\n            occupation: string\n            implements Named {}\n            implements Person {}\n        }\n        ",
        },
        "E0125",
    )
}

test "in_body_inherent_method_does_not_implicitly_satisfy_interface" {
    root.compiler.AssertHasCode(
        {
            "in_body_inherent_method_does_not_implicitly_satisfy_interface.baml": "\n        interface Label {\n            function label(self, name: string) -> string throws never\n        }\n\n        class Thing {\n            function label(self, value: string) -> string {\n                return value\n            }\n\n            implements Label {}\n        }\n        ",
        },
        "E0113",
    )
}

test "out_of_body_inherent_method_does_not_implicitly_satisfy_interface" {
    root.compiler.AssertHasCode(
        {
            "out_of_body_inherent_method_does_not_implicitly_satisfy_interface.baml": "\n        interface Label {\n            function label(self, name: string) -> string throws never\n        }\n\n        class Thing {\n            function label(self, value: string) -> string {\n                return value\n            }\n        }\n\n        implements Label for Thing {}\n        ",
        },
        "E0113",
    )
}

test "out_of_body_empty_implements_does_not_satisfy_abstract_via_inherent_method" {
    root.compiler.AssertHasCode(
        {
            "out_of_body_empty_implements_does_not_satisfy_abstract_via_inherent_method.baml": "\n        interface Printable {\n            function label(self) -> string throws never\n        }\n\n        class Box {\n            value: string\n\n            function label(self) -> string {\n                return self.value\n            }\n        }\n\n        implements Printable for Box {}\n        ",
        },
        "E0113",
    )
}

test "out_of_body_empty_implements_inherent_method_with_wrong_sig_does_not_satisfy" {
    root.compiler.AssertHasCode(
        {
            "out_of_body_empty_implements_inherent_method_with_wrong_sig_does_not_satisfy.baml": "\n        interface Printable {\n            function label(self) -> string throws never\n        }\n\n        class Box {\n            function label(self) -> int {\n                return 1\n            }\n        }\n\n        implements Printable for Box {}\n        ",
        },
        "E0113",
    )
}

test "out_of_body_implements_field_bearing_interface_is_error" {
    root.compiler.AssertHasCode(
        {
            "out_of_body_implements_field_bearing_interface_is_error.baml": "\n        interface Named {\n            name: string\n            function greet(self) -> string throws never\n        }\n        class Robot { model: string }\n        implements Named for Robot {\n            name: string\n            function greet(self) -> string { return \"I am \" + self.name }\n        }\n        ",
        },
        "E0126",
    )
}

test "out_of_body_implements_field_bearing_interface_is_error_even_without_redeclared_fields" {
    root.compiler.AssertHasCode(
        {
            "out_of_body_implements_field_bearing_interface_is_error_even_without_redeclared_fields.baml": "\n        interface Named {\n            name: string\n            function greet(self) -> string throws never\n        }\n        class Robot { model: string }\n        implements Named for Robot {\n            function greet(self) -> string { return \"I am a robot\" }\n        }\n        ",
        },
        "E0126",
    )
}

test "out_of_body_implements_inherited_field_bearing_interface_requires_parent" {
    root.compiler.AssertHasCode(
        {
            "out_of_body_implements_inherited_field_bearing_interface_requires_parent.baml": "\n        interface Named {\n            name: string\n        }\n        interface Child requires Named {}\n        class Robot { model: string }\n\n        implements Child for Robot {}\n        ",
        },
        "E0125",
    )
}

test "out_of_body_and_in_body_for_same_interface_is_error" {
    root.compiler.AssertHasCode(
        {
            "out_of_body_and_in_body_for_same_interface_is_error.baml": "\n        interface ToJson {\n            function to_json(self) -> string throws never\n        }\n        class Dog {\n            implements ToJson {\n                function to_json(self) -> string { return \"woof\" }\n            }\n        }\n        implements ToJson for Dog {\n            function to_json(self) -> string { return \"bark\" }\n        }\n        ",
        },
        "E0132",
    )
}

test "out_of_body_implements_for_generic_function_type_is_rejected" {
    root.compiler.AssertHasMessage(
        {
            "out_of_body_implements_for_generic_function_type_is_rejected.baml": "\n        interface GenericCallable {}\n        implements GenericCallable for <T>(x: int) -> int {}\n        ",
        },
        "generic parameters",
    )
}

test "generic_required_parent_args_must_match" {
    root.compiler.AssertHasCode(
        {
            "generic_required_parent_args_must_match.baml": "\n        interface Parent<T> {}\n        interface Child<T> requires Parent<T> {}\n\n        class Box {\n            implements Parent<string> {}\n            implements Child<int> {}\n        }\n        ",
        },
        "E0125",
    )
}

test "out_of_body_generic_required_parent_args_must_match" {
    root.compiler.AssertHasCode(
        {
            "out_of_body_generic_required_parent_args_must_match.baml": "\n        interface Parent<T> {}\n        interface Child<T> requires Parent<T> {}\n\n        implements Parent<string> for int {}\n        implements Child<int> for int {}\n        ",
        },
        "E0125",
    )
}

test "inherited_generic_interface_field_construction_uses_parent_args" {
    root.compiler.AssertHasMessage(
        {
            "inherited_generic_interface_field_construction_uses_parent_args.baml": "\n        interface Parent<T> {\n            value: T\n        }\n        interface Child<T> requires Parent<T> {}\n        class Box {\n            stored: int\n            implements Child<int> {}\n            implements Parent<int> {\n                value as stored\n            }\n        }\n        function main() -> void {\n            let b = Box { value: \"wrong\" }\n        }\n        ",
        },
        "expected `int`, found",
    )
}

test "bare_interface_method_value_requires_inferable_self" {
    root.compiler.AssertHasCode(
        {
            "bare_interface_method_value_requires_inferable_self.baml": "\n        interface Label<T> {\n            function label(self) -> string throws never {\n                return \"ok\"\n            }\n        }\n        class Box {\n            implements Label<int> {}\n        }\n        function main() -> void {\n            let label = Label.label\n        }\n        ",
        },
        "E0002",
    )
}

test "out_of_body_implements_for_primitive_cannot_add_fields" {
    root.compiler.AssertHasMessage(
        {
            "out_of_body_implements_for_primitive_cannot_add_fields.baml": "\n        interface Named {\n            name: string\n        }\n        implements Named for int {\n            name: string\n        }\n        ",
        },
        "field",
    )
}

test "cross_interface_assignment_is_compile_error" {
    root.compiler.AssertHasMessage(
        {
            "cross_interface_assignment_is_compile_error.baml": "\n        interface Animal {\n            function speak(self) -> string throws never\n        }\n        interface Swimmer {\n            function swim(self) -> string throws never\n        }\n        class Duck {\n            implements Animal {\n                function speak(self) -> string { return \"Quack!\" }\n            }\n            implements Swimmer {\n                function swim(self) -> string { return \"splash\" }\n            }\n        }\n        function bad() -> string {\n            let a: Animal = Duck {}\n            let s: Swimmer = a\n            return s.swim()\n        }\n        ",
        },
        "Animal",
    )
}

test "implements_for_union_target_is_rejected" {
    root.compiler.AssertHasCode(
        {
            "implements_for_union_target_is_rejected.baml": "\n        interface Tag {\n            function tag(self) -> int throws never\n        }\n        implements Tag for int | string {\n            function tag(self) -> int { return 0 }\n        }\n        ",
        },
        "E0138",
    )
}

test "implements_for_interface_target_is_rejected" {
    root.compiler.AssertHasCode(
        {
            "implements_for_interface_target_is_rejected.baml": "\n        interface Tag {\n            function tag(self) -> int throws never\n        }\n        interface Other {\n            function other(self) -> int throws never\n        }\n        implements Tag for Other {\n            function tag(self) -> int { return 0 }\n        }\n        ",
        },
        "E0138",
    )
}

test "implements_for_optional_target_is_rejected" {
    root.compiler.AssertHasCode(
        {
            "implements_for_optional_target_is_rejected.baml": "\n        interface Label {\n            function label(self) -> string throws never\n        }\n        implements<T> Label for T? {\n            function label(self) -> string { return \"optional\" }\n        }\n        ",
        },
        "E0138",
    )
}

test "impl_generic_bound_must_be_an_interface" {
    root.compiler.AssertHasCode(
        {
            "impl_generic_bound_must_be_an_interface.baml": "\n        interface Printable {\n            function print(self) -> string throws never\n        }\n        class Widget {\n            name: string\n        }\n        implements<T extends Widget> Printable for T {\n            function print(self) -> string { return \"w\" }\n        }\n        ",
        },
        "E0145",
    )
}

test "implements_for_unknown_target_is_rejected" {
    root.compiler.AssertHasCode(
        {
            "implements_for_unknown_target_is_rejected.baml": "\n        interface Tag {\n            function tag(self) -> int throws never\n        }\n        implements Tag for unknown {\n            function tag(self) -> int { return 0 }\n        }\n        ",
        },
        "E0138",
    )
}

test "implements_for_literal_target_is_rejected" {
    root.compiler.AssertHasCode(
        {
            "implements_for_literal_target_is_rejected.baml": "\n        interface Tag {\n            function tag(self) -> int throws never\n        }\n        implement Tag for 1 {\n            function tag(self) -> int { return 0 }\n        }\n        ",
        },
        "E0138",
    )
}

test "implements_for_enum_variant_target_is_rejected" {
    root.compiler.AssertHasCode(
        {
            "implements_for_enum_variant_target_is_rejected.baml": "\n        enum Color { Red, Green, Blue }\n        interface Tag {\n            function tag(self) -> int throws never\n        }\n        implement Tag for Color.Red {\n            function tag(self) -> int { return 0 }\n        }\n        ",
        },
        "E0138",
    )
}

test "generic_rule_for_list_receiver_overlaps_concrete_list" {
    root.compiler.AssertHasCode(
        {
            "generic_rule_for_list_receiver_overlaps_concrete_list.baml": "\n        interface Label {\n            function label(self) -> string throws never\n        }\n        implements Label for int[] {\n            function label(self) -> string { return \"ints\" }\n        }\n        implements<T> Label for T[] {\n            function label(self) -> string { return \"list\" }\n        }\n        ",
        },
        "E0132",
    )
}

test "unified_rule_rejects_mismatched_generic_interface_arg" {
    root.compiler.AssertHasMessage(
        {
            "unified_rule_rejects_mismatched_generic_interface_arg.baml": "\n        interface Container<T> {\n            function get(self) -> T throws never\n        }\n        class Wrapper<T> {\n            value: T\n        }\n        implements<T> Container<T> for Wrapper<T> {\n            function get(self) -> T { return self.value }\n        }\n        function take(c: Container<string>) -> string {\n            return c.get()\n        }\n        function bad() -> string {\n            return take(Wrapper<int> { value: 42 })\n        }\n        ",
        },
        "Wrapper<int>",
    )
}

test "unified_rule_repeated_type_vars_reject_conflicting_args" {
    root.compiler.AssertHasMessage(
        {
            "unified_rule_repeated_type_vars_reject_conflicting_args.baml": "\n        interface Same {\n            function tag(self) -> string throws never\n        }\n        class Pair<L, R> {\n            left: L\n            right: R\n        }\n        implements<T> Same for Pair<T, T> {\n            function tag(self) -> string { return \"same\" }\n        }\n        function take(p: Same) -> string {\n            return p.tag()\n        }\n        function bad() -> string {\n            return take(Pair<int, string> { left: 7, right: \"nope\" })\n        }\n        ",
        },
        "Pair<int, string>",
    )
}

test "form2_does_not_apply_when_bound_not_satisfied" {
    root.compiler.AssertHasMessage(
        {
            "form2_does_not_apply_when_bound_not_satisfied.baml": "\n        interface Named {\n            name: string\n        }\n        interface Printable {\n            function display(self) -> string throws never\n        }\n        class Rock {\n            label: string\n        }\n        implements<T extends Named> Printable for T {\n            function display(self) -> string { return self.name }\n        }\n        function bad() -> string {\n            let item: Printable = Rock { label: \"igneous\" }\n            return item.display()\n        }\n        ",
        },
        "Rock",
    )
}

test "bounded_generic_function_type_annotation_is_rejected" {
    root.compiler.AssertHasMessage(
        {
            "bounded_generic_function_type_annotation_is_rejected.baml": "\n        interface MyInterface {\n            function myMethod(self) -> int throws never\n        }\n        function main() -> void {\n            let method : <T extends MyInterface>(T) -> int = MyInterface.myMethod\n        }\n    ",
        },
        "generic parameters",
    )
}

test "parenthesized_generic_apply_is_rejected" {
    root.compiler.AssertHasMessage(
        {
            "parenthesized_generic_apply_is_rejected.baml": "\n        function identity<T>(x: T) -> T { x }\n        function caller() -> string {\n            let f = (identity)<int>\n            return \"ok\"\n        }\n        ",
        },
        "function reference",
    )
}

test "inferred_interface_method_reference_rejects_receiver_outside_bound" {
    root.compiler.AssertHasMessage(
        {
            "inferred_interface_method_reference_rejects_receiver_outside_bound.baml": "\n        interface MyInterface {\n            function myMethod(self) -> int throws never\n        }\n        class Other {}\n        function main() -> int {\n            let method = MyInterface.myMethod\n            return method(Other {})\n        }\n    ",
        },
        "MyInterface",
    )
}

test "overlapping_concrete_and_generic_rules_are_e0132" {
    root.compiler.AssertHasCode(
        {
            "overlapping_concrete_and_generic_rules_are_e0132.baml": "\n        interface Printable {\n            function display(self) -> string throws never\n        }\n        class Box<T> {\n            value: T\n        }\n        implements<T> Printable for Box<T> {\n            function display(self) -> string { return \"box\" }\n        }\n        implements Printable for Box<int> {\n            function display(self) -> string { return \"int box\" }\n        }\n        ",
        },
        "E0132",
    )
}

test "overlapping_generic_rules_are_e0132" {
    root.compiler.AssertHasCode(
        {
            "overlapping_generic_rules_are_e0132.baml": "\n        interface Printable {\n            function display(self) -> string throws never\n        }\n        class Box<T> {\n            value: T\n        }\n        implements<T> Printable for Box<T> {\n            function display(self) -> string { return \"box\" }\n        }\n        implements<U> Printable for Box<U> {\n            function display(self) -> string { return \"other box\" }\n        }\n        ",
        },
        "E0132",
    )
}

test "overlapping_complementary_generic_args_are_e0132" {
    root.compiler.AssertHasCode(
        {
            "overlapping_complementary_generic_args_are_e0132.baml": "\n        interface Printable {\n            function display(self) -> string throws never\n        }\n        class Pair<A, B> {\n            first: A\n            second: B\n        }\n        implements<T> Printable for Pair<T, int> {\n            function display(self) -> string { return \"t,int\" }\n        }\n        implements<U> Printable for Pair<string, U> {\n            function display(self) -> string { return \"string,u\" }\n        }\n        ",
        },
        "E0132",
    )
}

test "overlapping_in_body_and_out_of_body_generic_rules_are_e0132" {
    root.compiler.AssertHasCode(
        {
            "overlapping_in_body_and_out_of_body_generic_rules_are_e0132.baml": "\n        interface Printable {\n            function display(self) -> string throws never\n        }\n        class Box<T> {\n            value: T\n            implements Printable {\n                function display(self) -> string { return \"in body\" }\n            }\n        }\n        implements<T> Printable for Box<T> {\n            function display(self) -> string { return \"out of body\" }\n        }\n        ",
        },
        "E0132",
    )
}

test "cross_file_overlapping_impls_are_e0132" {
    root.compiler.AssertHasCode(
        {
            "a.baml": "\n                interface Printable {\n                    function display(self) -> string throws never\n                }\n                class Box<T> { value: T }\n                implements<T> Printable for Box<T> {\n                    function display(self) -> string { return \"box\" }\n                }\n                ",
            "b.baml": "\n                implements Printable for Box<int> {\n                    function display(self) -> string { return \"int box\" }\n                }\n                ",
        },
        "E0132",
    )
}

test "form2_overlap_with_form2_is_e0132" {
    root.compiler.AssertHasCode(
        {
            "form2_overlap_with_form2_is_e0132.baml": "\n        interface Named {\n            name: string\n        }\n        interface Printable {\n            function display(self) -> string throws never\n        }\n        implements<T extends Named> Printable for T {\n            function display(self) -> string { return \"first\" }\n        }\n        implements<U extends Named> Printable for U {\n            function display(self) -> string { return \"second\" }\n        }\n        ",
        },
        "E0132",
    )
}

test "overlapping_bounded_generic_receiver_rules_are_e0132" {
    root.compiler.AssertHasCode(
        {
            "overlapping_bounded_generic_receiver_rules_are_e0132.baml": "\n        interface Named {\n            name: string\n        }\n        interface Tagged {\n            tag: string\n        }\n        interface Printable {\n            function display(self) -> string throws never\n        }\n        class Box<T> {\n            value: T\n        }\n        implements<T extends Named> Printable for Box<T> {\n            function display(self) -> string { return \"named box\" }\n        }\n        implements<U extends Tagged> Printable for Box<U> {\n            function display(self) -> string { return \"tagged box\" }\n        }\n        ",
        },
        "E0132",
    )
}

test "overlapping_bounded_and_unbounded_generic_receiver_rules_are_e0132" {
    root.compiler.AssertHasCode(
        {
            "overlapping_bounded_and_unbounded_generic_receiver_rules_are_e0132.baml": "\n        interface Named {\n            name: string\n        }\n        interface Printable {\n            function display(self) -> string throws never\n        }\n        class Box<T> {\n            value: T\n        }\n        implements<T> Printable for Box<T> {\n            function display(self) -> string { return \"box\" }\n        }\n        implements<U extends Named> Printable for Box<U> {\n            function display(self) -> string { return \"named box\" }\n        }\n        ",
        },
        "E0132",
    )
}

test "bounded_type_var_rule_overlaps_concrete_satisfying_bound" {
    root.compiler.AssertHasCode(
        {
            "bounded_type_var_rule_overlaps_concrete_satisfying_bound.baml": "\n        interface Named {\n            name: string\n        }\n        interface Printable {\n            function display(self) -> string throws never\n        }\n        class User {\n            name: string\n            implements Named {}\n        }\n        implements Printable for User {\n            function display(self) -> string { return \"user\" }\n        }\n        implements<T extends Named> Printable for T {\n            function display(self) -> string { return \"named\" }\n        }\n        ",
        },
        "E0132",
    )
}

test "distinct_bounded_blankets_overlap_e0132" {
    root.compiler.AssertHasCode(
        {
            "distinct_bounded_blankets_overlap_e0132.baml": "\n        interface A { a: int }\n        interface B { b: int }\n        interface Printable { function display(self) -> string throws never }\n        implements<T extends A> Printable for T {\n            function display(self) -> string { return \"a\" }\n        }\n        implements<T extends B> Printable for T {\n            function display(self) -> string { return \"b\" }\n        }\n        ",
        },
        "E0132",
    )
}

test "unbounded_blanket_overlaps_concrete_e0132" {
    root.compiler.AssertHasCode(
        {
            "unbounded_blanket_overlaps_concrete_e0132.baml": "\n        interface Printable { function display(self) -> string throws never }\n        class Widget {}\n        implements<T> Printable for T {\n            function display(self) -> string { return \"any\" }\n        }\n        implements Printable for Widget {\n            function display(self) -> string { return \"widget\" }\n        }\n        ",
        },
        "E0132",
    )
}

test "orphan_rejects_foreign_interface_for_local_array" {
    root.compiler.AssertHasCode(
        {
            "orphan_rejects_foreign_interface_for_local_array.baml": "\n        class Local {}\n        implement baml.ops.Equals for Local[] {}\n        ",
        },
        "E0139",
    )
}

test "orphan_rejects_foreign_interface_for_map_of_local" {
    root.compiler.AssertHasCode(
        {
            "orphan_rejects_foreign_interface_for_map_of_local.baml": "\n        class Local {}\n        implement baml.ops.Equals for map<string, Local> {}\n        ",
        },
        "E0139",
    )
}

test "shared_var_across_interface_arg_and_for_type_overlaps_e0132" {
    root.compiler.AssertHasCode(
        {
            "shared_var_across_interface_arg_and_for_type_overlaps_e0132.baml": "\n        interface P<X> { function p(self) -> string throws never }\n        class Box<T> { v: T }\n        implements<T> P<T> for Box<T> {\n            function p(self) -> string { return \"t\" }\n        }\n        implements<U> P<int> for Box<U> {\n            function p(self) -> string { return \"int\" }\n        }\n        ",
        },
        "E0132",
    )
}

test "variable_in_union_arg_overlaps_via_union_expansion" {
    root.compiler.AssertHasMessage(
        {
            "variable_in_union_arg_overlaps_via_union_expansion.baml": "\n        interface Marker {}\n        class Box<X> { v: X }\n        implements<T> Marker for Box<int | T> {}\n        implements Marker for Box<int | string | bool> {}\n        ",
        },
        "overlapping interface implementations",
    )
}

test "definite_overlap_reports_overlapping_message" {
    root.compiler.AssertHasMessage(
        {
            "definite_overlap_reports_overlapping_message.baml": "\n        interface Marker {}\n        class Widget {}\n        implements<T> Marker for T {}\n        implements Marker for Widget {}\n        ",
        },
        "overlapping interface implementations",
    )
}

test "nested_generic_complementary_args_overlap_e0132" {
    root.compiler.AssertHasCode(
        {
            "nested_generic_complementary_args_overlap_e0132.baml": "\n        interface Marker {}\n        class Box<X> { v: X }\n        class Pair<A, B> { first: A second: B }\n        implements<T> Marker for Box<Pair<T, int>> {}\n        implements<U> Marker for Box<Pair<string, U>> {}\n        ",
        },
        "E0132",
    )
}

test "nested_union_in_generic_arg_overlaps_via_expansion" {
    root.compiler.AssertHasCode(
        {
            "nested_union_in_generic_arg_overlaps_via_expansion.baml": "\n        interface Marker {}\n        class Box<X> { v: X }\n        class Pair<A, B> { first: A second: B }\n        implements<T> Marker for Box<Pair<int, string | T>> {}\n        implements Marker for Box<Pair<int, string | bool>> {}\n        ",
        },
        "E0132",
    )
}

test "deeply_nested_generic_with_list_overlaps_e0132" {
    root.compiler.AssertHasCode(
        {
            "deeply_nested_generic_with_list_overlaps_e0132.baml": "\n        interface Marker {}\n        class Box<X> { v: X }\n        class Pair<A, B> { first: A second: B }\n        implements<T> Marker for Box<Pair<T[], int>> {}\n        implements<U> Marker for Box<Pair<string[], U>> {}\n        ",
        },
        "E0132",
    )
}

test "union_collapse_overlaps_e0132" {
    root.compiler.AssertHasCode(
        {
            "union_collapse_overlaps_e0132.baml": "\n        interface Marker {}\n        class Box<X> { v: X }\n        class Foo { x: int }\n        class Bar { y: int }\n        implements<T, U, W> Marker for Box<T[] | U[] | W[]> {}\n        implements Marker for Box<Foo[] | Bar[]> {}\n        ",
        },
        "E0132",
    )
}

test "complete_bool_union_folds_and_overlaps_e0132" {
    root.compiler.AssertHasCode(
        {
            "complete_bool_union_folds_and_overlaps_e0132.baml": "\n        interface Marker {}\n        class Box<X> { v: X }\n        implements Marker for Box<true | false> {}\n        implements Marker for Box<bool> {}\n        ",
        },
        "E0132",
    )
}

test "complete_enum_union_folds_and_overlaps_e0132" {
    root.compiler.AssertHasCode(
        {
            "complete_enum_union_folds_and_overlaps_e0132.baml": "\n        interface Marker {}\n        class Box<X> { v: X }\n        enum Cmp { Less Equal More }\n        implements Marker for Box<Cmp.Less | Cmp.Equal | Cmp.More> {}\n        implements Marker for Box<Cmp> {}\n        ",
        },
        "E0132",
    )
}

test "intractable_union_overlap_asks_to_simplify" {
    root.compiler.AssertHasMessage(
        {
            "intractable_union_overlap_asks_to_simplify.baml": "\n        interface Marker {}\n        class Box<X> { v: X }\n        class Pair<A, B> { f: A s: B }\n        class A0 { x: int } class A1 { x: int } class A2 { x: int }\n        class A3 { x: int } class A4 { x: int } class A5 { x: int }\n        implements<T0, T1, T2, T3, T4> Marker for Box<\n            Pair<T0, T0> | Pair<T1, T1> | Pair<T2, T2> | Pair<T3, T3> | Pair<T4, T4>\n        > {}\n        implements Marker for Box<\n            Pair<A0, A0> | Pair<A1, A1> | Pair<A2, A2> | Pair<A3, A3> | Pair<A4, A4> | Pair<A5, A5>\n        > {}\n        ",
        },
        "simplify",
    )
}

test "union_member_vs_single_collapse_overlaps_e0132" {
    root.compiler.AssertHasCode(
        {
            "union_member_vs_single_collapse_overlaps_e0132.baml": "\n        interface Marker {}\n        class Box<X> { v: X }\n        class C {}\n        implements<T> Marker for Box<C | T> {}\n        implements Marker for Box<C> {}\n        ",
        },
        "E0132",
    )
}

test "box_unknown_vs_blanket_overlaps_e0132" {
    root.compiler.AssertHasCode(
        {
            "box_unknown_vs_blanket_overlaps_e0132.baml": "\n        interface Marker {}\n        class Box<X> { v: X }\n        implements<T> Marker for Box<T> {}\n        implements Marker for Box<unknown> {}\n        ",
        },
        "E0132",
    )
}

test "bounded_union_member_overlaps_concrete_union_e0132" {
    root.compiler.AssertHasCode(
        {
            "bounded_union_member_overlaps_concrete_union_e0132.baml": "\n        interface HasName {}\n        interface Marker {}\n        class Box<X> { v: X }\n        class Cat {}\n        class Dog {}\n        implements HasName for Dog {}\n        implements<T extends HasName, R> Marker for Box<T[] | R> {}\n        implements Marker for Box<Cat[] | Dog[]> {}\n        ",
        },
        "E0132",
    )
}

test "bounded_union_member_overlaps_concrete_union_reversed_e0132" {
    root.compiler.AssertHasCode(
        {
            "bounded_union_member_overlaps_concrete_union_reversed_e0132.baml": "\n        interface HasName {}\n        interface Marker {}\n        class Box<X> { v: X }\n        class Cat {}\n        class Dog {}\n        implements HasName for Dog {}\n        implements<T extends HasName, R> Marker for Box<T[] | R> {}\n        implements Marker for Box<Dog[] | Cat[]> {}\n        ",
        },
        "E0132",
    )
}

test "alias_for_type_overlaps_concrete_e0132" {
    root.compiler.AssertHasCode(
        {
            "alias_for_type_overlaps_concrete_e0132.baml": "\n        interface Marker {}\n        class C {}\n        type AliasC = C\n        implement Marker for C {}\n        implement Marker for AliasC {}\n        ",
        },
        "E0132",
    )
}

test "alias_of_alias_for_type_overlaps_e0132" {
    root.compiler.AssertHasCode(
        {
            "alias_of_alias_for_type_overlaps_e0132.baml": "\n        interface Marker {}\n        class C {}\n        type A = C\n        type B = A\n        implement Marker for C {}\n        implement Marker for B {}\n        ",
        },
        "E0132",
    )
}

test "two_distinct_aliases_of_same_class_overlap_e0132" {
    root.compiler.AssertHasCode(
        {
            "two_distinct_aliases_of_same_class_overlap_e0132.baml": "\n        interface Marker {}\n        class C {}\n        type A = C\n        type B = C\n        implement Marker for A {}\n        implement Marker for B {}\n        ",
        },
        "E0132",
    )
}

test "alias_for_type_overlaps_blanket_e0132" {
    root.compiler.AssertHasCode(
        {
            "alias_for_type_overlaps_blanket_e0132.baml": "\n        interface Marker {}\n        class Box<X> { v: X }\n        type BI = Box<int>\n        implements<T> Marker for Box<T> {}\n        implement Marker for BI {}\n        ",
        },
        "E0132",
    )
}

test "future_for_type_is_rejected_e0138" {
    root.compiler.AssertHasCode(
        {
            "future_for_type_is_rejected_e0138.baml": "\n        interface Marker {}\n        implement Marker for baml.future.Future<int, string> {}\n        ",
        },
        "E0138",
    )
}

test "bounded_type_var_rule_conservatively_overlaps_generic_class_rule" {
    root.compiler.AssertHasCode(
        {
            "bounded_type_var_rule_conservatively_overlaps_generic_class_rule.baml": "\n        interface Named {\n            name: string\n        }\n        interface Printable {\n            function display(self) -> string throws never\n        }\n        class Box<T> {\n            value: T\n        }\n        implements<T> Printable for Box<T> {\n            function display(self) -> string { return \"box\" }\n        }\n        implements<U extends Named> Printable for U {\n            function display(self) -> string { return \"named\" }\n        }\n        ",
        },
        "E0132",
    )
}

test "bounded_type_var_rule_conservatively_overlaps_in_body_generic_class_rule" {
    root.compiler.AssertHasCode(
        {
            "bounded_type_var_rule_conservatively_overlaps_in_body_generic_class_rule.baml": "\n        interface Named {\n            name: string\n        }\n        interface Printable {\n            function display(self) -> string throws never\n        }\n        class Box<T> {\n            value: T\n            implements Printable {\n                function display(self) -> string { return \"box\" }\n            }\n        }\n        implements<U extends Named> Printable for U {\n            function display(self) -> string { return \"named\" }\n        }\n        ",
        },
        "E0132",
    )
}

test "fuzz_bug06_interface_requires_non_interface_errors_at_declaration" {
    root.compiler.AssertHasMessage(
        {
            "fuzz_bug06_interface_requires_non_interface_errors_at_declaration.baml": "\n        class Foo { x: int }\n        interface Bad requires Foo {}\n        function main() -> bool { return true }\n",
        },
        "Foo",
    )
}

test "fuzz_bug08_generic_class_overlapping_type_arg_impls_rejected" {
    root.compiler.AssertHasCode(
        {
            "fuzz_bug08_generic_class_overlapping_type_arg_impls_rejected.baml": "interface Getter<T> {\n    function get(self) -> T throws never\n}\n\nclass Pair<L, R> {\n    left: L\n    right: R\n    implements Getter<L> {\n        function get(self) -> L { return self.left }\n    }\n    implements Getter<R> {\n        function get(self) -> R { return self.right }\n    }\n}\n\nfunction main() -> bool {\n    let p: Pair<int, string> = Pair { left: 7, right: \"seven\" }\n    let gr: Getter<string> = p\n    let val = gr.get()\n    return val == \"seven\"\n}\n",
        },
        "E0132",
    )
}

test "fuzz_bug09_same_generic_iface_diff_typeargs_unqualified_call_ambiguous" {
    root.compiler.AssertHasCode(
        {
            "fuzz_bug09_same_generic_iface_diff_typeargs_unqualified_call_ambiguous.baml": "interface Converter<T> {\n    function convert(self) -> T throws never\n}\n\nclass MultiFormat {\n    int_val: int\n    str_val: string\n    implements Converter<int> {\n        function convert(self) -> int { return self.int_val }\n    }\n    implements Converter<string> {\n        function convert(self) -> string { return self.str_val }\n    }\n}\n\nfunction main() -> int {\n    let m = MultiFormat { int_val: 42, str_val: \"hello\" }\n    return m.convert()\n}\n",
        },
        "E0121",
    )
}

test "fuzz_bug11_ambiguous_generic_field_error_includes_type_args" {
    root.compiler.AssertHasMessage(
        {
            "fuzz_bug11_ambiguous_generic_field_error_includes_type_args.baml": "interface Box<T> {\n    value: T\n}\n\nclass MultiBox {\n    int_v: int\n    str_v: string\n    implements Box<int> { value as int_v }\n    implements Box<string> { value as str_v }\n}\n\nfunction main() -> int {\n    let b = MultiBox { int_v: 42, str_v: \"hello\" }\n    return b.value\n}\n",
        },
        "as<Box<int>>",
    )
}

test "fuzz_bug12_generic_pair_overlapping_type_arg_impls_rejected" {
    root.compiler.AssertHasCode(
        {
            "fuzz_bug12_generic_pair_overlapping_type_arg_impls_rejected.baml": "interface Slot<T> {\n    function get(self) -> T throws never\n}\n\nclass GenPair<L, R> {\n    left: L\n    right: R\n    implements Slot<L> {\n        function get(self) -> L { return self.left }\n    }\n    implements Slot<R> {\n        function get(self) -> R { return self.right }\n    }\n}\n\nfunction main() -> bool {\n    let p: GenPair<int, string> = GenPair { left: 42, right: \"world\" }\n    let rl: Slot<int> = p\n    let rr: Slot<string> = p\n    let lv = rl.get()\n    let rv = rr.get()\n    // Each interface view dispatches to its own impl block:\n    // lv == 42 (Slot<int>) && rv == \"world\" (Slot<string>).\n    return lv == 42 && rv == \"world\"\n}\n",
        },
        "E0132",
    )
}

test "fuzz_bug13_generic_field_link_overlapping_type_arg_impls_rejected" {
    root.compiler.AssertHasCode(
        {
            "fuzz_bug13_generic_field_link_overlapping_type_arg_impls_rejected.baml": "interface Slot<T> {\n    value: T\n}\n\nclass GenPair<L, R> {\n    left: L\n    right: R\n    implements Slot<L> {\n        value as left\n    }\n    implements Slot<R> {\n        value as right\n    }\n}\n\nfunction main() -> bool {\n    let p: GenPair<int, string> = GenPair { left: 7, right: \"seven\" }\n    let i: Slot<int> = p\n    let s: Slot<string> = p\n    // Each field-link view selects the impl block matching its type arg:\n    // i.value == 7 (Slot<int>) && s.value == \"seven\" (Slot<string>).\n    return i.value == 7 && s.value == \"seven\"\n}\n",
        },
        "E0132",
    )
}

test "fuzz_bug14_generic_overlapping_type_arg_impls_reject_as_projection" {
    root.compiler.AssertHasCode(
        {
            "fuzz_bug14_generic_overlapping_type_arg_impls_reject_as_projection.baml": "interface Slot<T> {\n    function get(self) -> T throws never\n}\n\nclass GenPair<L, R> {\n    left: L\n    right: R\n    implements Slot<L> {\n        function get(self) -> L { return self.left }\n    }\n    implements Slot<R> {\n        function get(self) -> R { return self.right }\n    }\n}\n\nfunction main() -> bool {\n    let p: GenPair<int, string> = GenPair { left: 42, right: \"world\" }\n    // Explicit .as<> projection should select the Slot<string> block\n    let rv = p.as<Slot<string>>.get()\n    return rv == \"world\"\n}\n",
        },
        "E0132",
    )
}

test "fuzz_bug19_aliased_field_type_mismatch_error_names_interface_field" {
    root.compiler.AssertHasMessage(
        {
            "fuzz_bug19_aliased_field_type_mismatch_error_names_interface_field.baml": "interface Named {\n    name: string\n}\nclass Person {\n    name_count: int\n    implements Named {\n        name as name_count\n    }\n}\nfunction main() -> string {\n    return \"unreachable\"\n}\n",
        },
        "`name`",
    )
}

test "fuzz_bug22_old_projection_syntax_is_no_member_error" {
    root.compiler.AssertHasMessage(
        {
            "fuzz_bug22_old_projection_syntax_is_no_member_error.baml": "interface Container<T> {\n    function get(self) -> T throws never\n}\nclass IntBox {\n    value: int\n    implements Container<int> {\n        function get(self) -> int { return self.value }\n    }\n}\nfunction main() -> int {\n    let b = IntBox { value: 42 }\n    return b.Container.get()\n}\n",
        },
        "has no member `Container`",
    )
}

test "fuzz_bug25_two_same_named_default_methods_are_ambiguous" {
    root.compiler.AssertHasCode(
        {
            "fuzz_bug25_two_same_named_default_methods_are_ambiguous.baml": "interface Alpha {\n    function tag(self) -> string throws never { return \"alpha\" }\n}\ninterface Beta {\n    function tag(self) -> string throws never { return \"beta\" }\n}\n\nclass Both {\n    implements Alpha {}\n    implements Beta {}\n}\n\nfunction main() -> string {\n    let x = Both {}\n    return x.tag()\n}\n",
        },
        "E0121",
    )
}

test "fuzz_bug26_default_plus_required_same_name_is_ambiguous" {
    root.compiler.AssertHasCode(
        {
            "fuzz_bug26_default_plus_required_same_name_is_ambiguous.baml": "interface WithDefault {\n    function process(self) -> string throws never { return \"DEFAULT\" }\n}\ninterface WithRequired {\n    function process(self) -> string throws never\n}\n\nclass Impl {\n    implements WithDefault {}\n    implements WithRequired {\n        function process(self) -> string { return \"REQUIRED\" }\n    }\n}\n\nfunction main() -> string {\n    let x = Impl {}\n    return x.process()  // E0121: ambiguous between the default and required `process`\n}\n",
        },
        "E0121",
    )
}

test "fuzz_bug27_same_generic_iface_diff_typeargs_no_receiver_is_ambiguous" {
    root.compiler.AssertHasCode(
        {
            "fuzz_bug27_same_generic_iface_diff_typeargs_no_receiver_is_ambiguous.baml": "interface Converter<T> {\n    function convert(self) -> T throws never\n}\n\nclass Multi {\n    implements Converter<int> {\n        function convert(self) -> int { return 42 }\n    }\n    implements Converter<string> {\n        function convert(self) -> string { return \"hello\" }\n    }\n}\n\nfunction main() -> int {\n    let m = Multi {}\n    return m.convert()  // E0121: ambiguous between Converter<int> and Converter<string>\n}\n",
        },
        "E0121",
    )
}

test "fuzz_bug28_ambiguous_generic_field_error_lists_type_args" {
    root.compiler.AssertHasMessage(
        {
            "fuzz_bug28_ambiguous_generic_field_error_lists_type_args.baml": "interface Slot<T> {\n    value: T\n}\n\nclass Pair {\n    int_value: int\n    str_value: string\n    implements Slot<int> { value as int_value }\n    implements Slot<string> { value as str_value }\n}\n\nfunction main() -> string {\n    let p = Pair { int_value: 1, str_value: \"hi\" }\n    return p.value\n}\n",
        },
        "Slot<int>",
    )
}

test "requires_real_non_interface_is_still_non_interface_error" {
    root.compiler.AssertHasCode(
        {
            "requires_real_non_interface_is_still_non_interface_error.baml": "\n        class RealClass { x: int }\n        interface Person requires RealClass {\n            name: string\n        }\n        ",
        },
        "E0133",
    )
}

test "wf3_generic_default_method_overlapping_type_arg_impls_rejected" {
    root.compiler.AssertHasCode(
        {
            "wf3_generic_default_method_overlapping_type_arg_impls_rejected.baml": "\n        interface Slot<T> {\n            function get(self) -> T throws never\n            function describe(self) -> T throws never {\n                return self.get()\n            }\n        }\n        class GenPair<L, R> {\n            left: L\n            right: R\n            implements Slot<L> {\n                function get(self) -> L { return self.left }\n            }\n            implements Slot<R> {\n                function get(self) -> R { return self.right }\n            }\n        }\n        function main() -> string {\n            let p: GenPair<int, string> = GenPair { left: 7, right: \"seven\" }\n            let s: Slot<string> = p\n            return s.describe()\n        }\n        ",
        },
        "E0132",
    )
}

test "wf3_monomorph_collision_overlapping_impls_rejected" {
    root.compiler.AssertHasMessage(
        {
            "wf3_monomorph_collision_overlapping_impls_rejected.baml": "\n        interface Getter<T> {\n            function get(self) -> T throws never\n        }\n        class Pair<L, R> {\n            left: L\n            right: R\n            implements Getter<L> {\n                function get(self) -> L { return self.left }\n            }\n            implements Getter<R> {\n                function get(self) -> R { return self.right }\n            }\n        }\n        function main() -> int {\n            let p: Pair<int, int> = Pair { left: 7, right: 99 }\n            return p.get()\n        }\n        ",
        },
        "overlapping interface implementations",
    )
}

test "wf3_unsatisfied_blanket_bound_message_names_bound" {
    root.compiler.AssertHasMessage(
        {
            "wf3_unsatisfied_blanket_bound_message_names_bound.baml": "\n        interface Named { name: string }\n        interface Printable { function display(self) -> string throws never }\n        class Rock { label: string }\n        class Box<T> { value: T }\n        implements<T extends Named> Printable for Box<T> {\n            function display(self) -> string { return self.value.name }\n        }\n        function main() -> string {\n            let item: Printable = Box<Rock> { value: Rock { label: \"ig\" } }\n            return item.display()\n        }\n        ",
        },
        "Named",
    )
}

test "wf3_as_to_unimplemented_interface_message_mentions_implement" {
    root.compiler.AssertHasMessage(
        {
            "wf3_as_to_unimplemented_interface_message_mentions_implement.baml": "\n        interface Animal {\n            function speak(self) -> string throws never\n        }\n        interface Vehicle {\n            function drive(self) -> string throws never\n        }\n        class Cat {\n            implements Animal {\n                function speak(self) -> string { return \"Meow.\" }\n            }\n        }\n        function main() -> string {\n            let c = Cat {}\n            return c.as<Vehicle>.drive()\n        }\n        ",
        },
        "implement",
    )
}

test "wf3_default_as_bare_value_is_rejected" {
    root.compiler.AssertHasMessage(
        {
            "wf3_default_as_bare_value_is_rejected.baml": "\n        interface Logger {\n            function log(self, msg: string) -> string throws never { return msg }\n        }\n        class C {\n            implements Logger {\n                function log(self, msg: string) -> string {\n                    let x = default\n                    return msg\n                }\n            }\n        }\n        function main() -> string {\n            let c = C {}\n            return c.log(\"hi\")\n        }\n        ",
        },
        "default",
    )
}

test "wf3_self_interface_field_access_in_method_requires_projection" {
    root.compiler.AssertHasMessage(
        {
            "wf3_self_interface_field_access_in_method_requires_projection.baml": "\n        interface Named {\n            name: string\n            function greet(self) -> string throws never\n        }\n        class Person {\n            title: string\n            implements Named {\n                name as title\n                function greet(self) -> string {\n                    return \"Hi \" + self.name\n                }\n            }\n        }\n        ",
        },
        "as<Named>",
    )
}

test "wf3_all_default_requires_parent_still_needs_explicit_implements" {
    root.compiler.AssertHasCode(
        {
            "wf3_all_default_requires_parent_still_needs_explicit_implements.baml": "\n        interface A {\n            function greet(self) -> string throws never { return \"hi\" }\n        }\n        interface B requires A {\n            function bye(self) -> string throws never { return \"bye\" }\n        }\n        class C {\n            implements B {}\n        }\n        function main() -> string {\n            let c: B = C {}\n            return c.bye()\n        }\n        ",
        },
        "E0125",
    )
}

test "wf3_requires_cycle_reports_full_path" {
    root.compiler.AssertHasMessage(
        {
            "wf3_requires_cycle_reports_full_path.baml": "\n        interface A requires B { function fa(self) -> string throws never }\n        interface B requires A { function fb(self) -> string throws never }\n        class C {\n            implements A { function fa(self) -> string { return \"a\" } }\n            implements B { function fb(self) -> string { return \"b\" } }\n        }\n        function main() -> string {\n            let c: A = C {}\n            return c.fa()\n        }\n        ",
        },
        "A → B",
    )
}

test "wf3_out_of_body_primitive_field_bearing_is_e0126_pins" {
    root.compiler.AssertHasCode(
        {
            "wf3_out_of_body_primitive_field_bearing_is_e0126_pins.baml": "\n        interface Named {\n            name: string\n            function display(self) -> string throws never\n        }\n        implements Named for int {\n            function display(self) -> string { return \"int\" }\n        }\n        function main() -> string { return \"ok\" }\n        ",
        },
        "E0126",
    )
}

test "union_fuzz_class_only_union_method_is_rejected" {
    root.compiler.AssertHasMessage(
        {
            "union_fuzz_class_only_union_method_is_rejected.baml": "\n        class A {\n          name: string\n          function execute(self) -> string { return \"A executes and gets \" + self.name }\n        }\n        class B {\n          name: string\n          function execute(self) -> string { return \"B executes and gets \" + self.name }\n        }\n        function process(input: A | B) -> string { return input.execute() }\n        function main() -> string {\n          return process(A { name: \"Alice\" }) + \" | \" + process(B { name: \"Bob\" })\n        }\n        ",
        },
        "no common interface that declares",
    )
}

test "union_ruling_mixed_inherent_and_impl_arm_is_rejected" {
    root.compiler.AssertHasMessage(
        {
            "union_ruling_mixed_inherent_and_impl_arm_is_rejected.baml": "\n        interface HudSpeaker {\n            function speak(self) -> string throws never\n        }\n        class HudA {\n            implements HudSpeaker {\n                function speak(self) -> string { return \"interface\" }\n            }\n        }\n        class HudB {\n            function speak(self) -> string { return \"inherent\" }\n        }\n        function hud_speak(x: HudA | HudB) -> string {\n            return x.speak()\n        }\n        function main() -> string {\n            return hud_speak(HudB {})\n        }\n        ",
        },
        "no common interface that declares",
    )
}

test "same_head_impls_differing_only_in_bounds_are_rejected_as_overlapping" {
    root.compiler.AssertHasCode(
        {
            "same_head_impls_differing_only_in_bounds_are_rejected_as_overlapping.baml": "\n        interface CbdMarkA {\n            function ma(self) -> int throws never\n        }\n        interface CbdMarkB {\n            function mb(self) -> int throws never\n        }\n        interface CbdConv {\n            function conv(self) -> int throws never\n        }\n        class CbdBox<T> {\n            inner: T\n        }\n        implements<T extends CbdMarkA> CbdConv for CbdBox<T> {\n            function conv(self) -> int throws never { return 1 }\n        }\n        implements<T extends CbdMarkB> CbdConv for CbdBox<T> {\n            function conv(self) -> int throws never { return 2 }\n        }\n        ",
        },
        "E0132",
    )
}

test "default_bypass_of_required_method_is_rejected" {
    root.compiler.AssertHasMessage(
        {
            "default_bypass_of_required_method_is_rejected.baml": "\n        interface DreqIface {\n            function must(self) -> string throws never\n            function wrap(self) -> string throws never { \"w\" }\n        }\n        class DreqHolder {\n            implements DreqIface {\n                function must(self) -> string throws never { \"m\" }\n                function wrap(self) -> string throws never { \"x \" + default.must() }\n            }\n        }\n        function dreq_probe(h: DreqHolder) -> string throws never {\n            h.wrap()\n        }\n        ",
        },
        "unresolved name: `default.must`",
    )
}

test "union_fuzz_pr_shared_implementor_method_in_iface_union_has_no_common_interface" {
    root.compiler.AssertHasMessage(
        {
            "union_fuzz_pr_shared_implementor_method_in_iface_union_has_no_common_interface.baml": "\n        interface A { function m(self) -> string  throws never { return \"from-A\" } }\n        interface B { function m(self) -> string  throws never { return \"from-B\" } }\n        class C { implements A {} implements B {} }\n        function call(x: A | B) -> string { return x.m() }\n        function main() -> string { let c: A = C {}; return call(c) }\n        ",
        },
        "no common interface that declares",
    )
}

test "union_fuzz_pr_disjoint_implementors_iface_union_is_rejected" {
    root.compiler.AssertHasMessage(
        {
            "union_fuzz_pr_disjoint_implementors_iface_union_is_rejected.baml": "\n        interface A { function m(self) -> string  throws never { return \"a\" } }\n        interface B { function m(self) -> string  throws never { return \"b\" } }\n        class Dog { implements A {} }\n        class Cat { implements B {} }\n        function call(x: A | B) -> string { return x.m() }\n        function main() -> string {\n          let d: A = Dog {}\n          let c: B = Cat {}\n          return call(d) + call(c)\n        }\n        ",
        },
        "no common interface that declares",
    )
}

test "union_fuzz_f03_conflicting_union_field_has_no_common_interface" {
    root.compiler.AssertHasMessage(
        {
            "union_fuzz_f03_conflicting_union_field_has_no_common_interface.baml": "\n        interface Animal { id: string }\n        interface Vehicle { id: int }\n        class Dog { id: string  implements Animal {} }\n        class Car { id: int  implements Vehicle {} }\n        function pick(b: bool) -> Animal | Vehicle {\n          if b { return Dog { id: \"rex\" } } else { return Car { id: 7 } }\n        }\n        function main() -> string {\n          let u: Animal | Vehicle = pick(false)\n          let v: string = u.id\n          return v\n        }\n        ",
        },
        "no common interface that declares",
    )
}

test "union_fuzz_f07_shared_method_on_iface_union_is_a_compile_error" {
    root.compiler.AssertHasMessage(
        {
            "union_fuzz_f07_shared_method_on_iface_union_is_a_compile_error.baml": "\n        interface Animal {\n          function speak(self) -> string throws never\n        }\n        class Dog {\n          implements Animal {\n            function speak(self) -> string { return \"Woof\" }\n          }\n        }\n        interface Vehicle {\n          function speak(self) -> string throws never\n        }\n        class Car {\n          implements Vehicle {\n            function speak(self) -> string { return \"Vroom\" }\n          }\n        }\n        function main() -> string {\n          let v: Animal | Vehicle = Dog {};\n          return v.speak();\n        }\n        ",
        },
        "no common interface that declares",
    )
}

test "union_fuzz_f11_conflicting_union_field_read_is_rejected" {
    root.compiler.AssertHasMessage(
        {
            "union_fuzz_f11_conflicting_union_field_read_is_rejected.baml": "\n        interface Animal { id: string }\n        interface Vehicle { id: int }\n        class Dog { id: string  implements Animal {} }\n        class Car { id: int  implements Vehicle {} }\n        function readBool(u: Animal | Vehicle) -> bool {\n          let v: bool = u.id\n          return v\n        }\n        function main() -> bool { return readBool(Dog { id: \"x\" }) }\n        ",
        },
        "no common interface that declares",
    )
}

test "union_fuzz_f16_unqualified_ambiguous_union_field_is_e0131" {
    root.compiler.AssertHasCode(
        {
            "union_fuzz_f16_unqualified_ambiguous_union_field_is_e0131.baml": "\n        interface Named { name: string }\n        interface Labeled { name: string }\n        class Person {\n          full: string\n          handle: string\n          implements Named { name as full }\n          implements Labeled { name as handle }\n        }\n        class Company {\n          legal: string\n          brand: string\n          implements Named { name as legal }\n          implements Labeled { name as brand }\n        }\n        function rawName(x: Person | Company) -> string {\n          return x.name\n        }\n        function main() -> string {\n          return rawName(Person { full: \"Ada\", handle: \"ada99\" })\n        }\n        ",
        },
        "E0131",
    )
}

test "union_fuzz_f17_projection_failure_names_full_generic_interface" {
    root.compiler.AssertHasMessage(
        {
            "union_fuzz_f17_projection_failure_names_full_generic_interface.baml": "\n        interface Cargo<T> { payload: T }\n        class Box<T> {\n          inner: T\n          implements Cargo<T> { payload as inner }\n        }\n        function takeInt(x: Box<int> | Box<string>) -> int {\n          return x.as<Cargo<int>>.payload\n        }\n        function main() -> int {\n          let b: Box<int> | Box<string> = Box<string> { inner: \"oops\" }\n          return takeInt(b)\n        }\n        ",
        },
        "Cargo<int>",
    )
}

test "field_chain_abstract_interface_method_arity_checked" {
    root.compiler.AssertHasMessage(
        {
            "field_chain_abstract_interface_method_arity_checked.baml": "\n        interface Conv {\n            function convert<T>(self, v: T) -> T throws never\n        }\n        class Celsius {\n            tag: string\n            implements Conv {\n                function convert<T>(self, v: T) -> T { v }\n            }\n        }\n        class Holder {\n            c: Conv\n        }\n        function main() -> int {\n            let h = Holder { c: Celsius { tag: \"x\" } };\n            h.c.convert<int, string>(5)\n        }\n    ",
        },
        "type argument",
    )
}

test "duplicate_implements_differing_only_in_assoc_bindings_is_compile_error" {
    root.compiler.AssertHasCode(
        {
            "duplicate_implements_differing_only_in_assoc_bindings_is_compile_error.baml": "\n        interface Sink {\n            type Error\n            function push(self, v: int) -> string throws never\n        }\n\n        class Buf<E1, E2> {\n            tag: string\n            implements Sink {\n                type Error = E1\n                function push(self, v: int) -> string { \"a\" }\n            }\n            implements Sink {\n                type Error = E2\n                function push(self, v: int) -> string { \"b\" }\n            }\n        }\n        ",
        },
        "E0132",
    )
}

test "ordering_on_union_operands_is_rejected" {
    root.compiler.AssertHasMessage(
        {
            "ordering_on_union_operands_is_rejected.baml": "\n        function f(a: int | string, b: int | string) -> bool {\n            a < b\n        }\n        ",
        },
        "does not implement `Compare`",
    )
}

test "ordering_on_interface_existential_is_rejected" {
    root.compiler.AssertHasMessage(
        {
            "ordering_on_interface_existential_is_rejected.baml": "\n        function f(a: baml.ops.Compare, b: baml.ops.Compare) -> bool {\n            a < b\n        }\n        ",
        },
        "does not implement `Compare`",
    )
}

test "compare_bound_rejects_abstract_type_argument" {
    root.compiler.AssertHasCode(
        {
            "compare_bound_rejects_abstract_type_argument.baml": "\n        function max<T extends baml.ops.Compare>(a: T, b: T) -> T throws never {\n            if a < b { b } else { a }\n        }\n        function f(a: int | string, b: int | string) -> int | string throws never {\n            max<int | string>(a, b)\n        }\n        ",
        },
        "E0001",
    )
}

test "ordering_on_interface_existential_type_argument_is_rejected" {
    root.compiler.AssertHasCode(
        {
            "ordering_on_interface_existential_type_argument_is_rejected.baml": "\n        function max<T extends baml.ops.Compare>(a: T, b: T) -> T throws never {\n            if a < b { b } else { a }\n        }\n        function f(a: baml.ops.Compare, b: baml.ops.Compare) -> baml.ops.Compare throws never {\n            max<baml.ops.Compare>(a, b)\n        }\n        ",
        },
        "E0001",
    )
}

test "compare_without_equals_is_rejected" {
    root.compiler.AssertHasCode(
        {
            "compare_without_equals_is_rejected.baml": "\n        class NoEq {\n            v: int\n            implements baml.ops.Compare {\n                function cmp(self, other: Self) -> baml.ops.Ordering throws never {\n                    self.v.cmp(other.v)\n                }\n            }\n        }\n        ",
        },
        "E0125",
    )
}

test "arithmetic_on_user_type_without_impl_is_rejected" {
    root.compiler.AssertHasMessage(
        {
            "arithmetic_on_user_type_without_impl_is_rejected.baml": "\n        class Vec2 { x: int }\n        function f(a: Vec2, b: Vec2) -> Vec2 {\n            a + b\n        }\n        ",
        },
        "cannot be applied",
    )
}

test "negate_on_user_type_without_impl_is_rejected" {
    root.compiler.AssertHasMessage(
        {
            "negate_on_user_type_without_impl_is_rejected.baml": "\n        class Vec2 { x: int }\n        function f(a: Vec2) -> Vec2 {\n            -a\n        }\n        ",
        },
        "cannot be applied",
    )
}

test "arithmetic_on_existential_without_pinned_output_is_rejected" {
    root.compiler.AssertHasMessage(
        {
            "arithmetic_on_existential_without_pinned_output_is_rejected.baml": "\n        function f(x: baml.ops.Add<int>, a: int) -> int {\n            x + a\n        }\n        ",
        },
        "cannot be applied",
    )
}

test "negate_on_bounded_typevar_without_pinned_output_is_rejected" {
    root.compiler.AssertHasMessage(
        {
            "negate_on_bounded_typevar_without_pinned_output_is_rejected.baml": "\n        function f<T extends baml.ops.Negate>(x: T) -> int {\n            -x;\n            0\n        }\n        ",
        },
        "cannot be applied",
    )
}

test "arithmetic_on_bounded_typevar_without_pinned_output_is_rejected" {
    root.compiler.AssertHasMessage(
        {
            "arithmetic_on_bounded_typevar_without_pinned_output_is_rejected.baml": "\n        function f<T extends baml.ops.Add<int>>(x: T) -> int {\n            x + 1;\n            0\n        }\n        ",
        },
        "cannot be applied",
    )
}

test "compound_assign_result_not_assignable_to_target_is_rejected" {
    root.compiler.AssertHasMessage(
        {
            "compound_assign_result_not_assignable_to_target_is_rejected.baml": "\n        class Counter {\n            n: int\n            implements baml.ops.Add<int> {\n                type Output = int\n                function add(self, rhs: int) -> int throws never { self.n + rhs }\n            }\n        }\n        function f(c: Counter) -> Counter {\n            c += 1;\n            c\n        }\n        ",
        },
        "mismatched types",
    )
}

// BEGIN CUSTOM DIAGNOSTIC PORTS

test "diagnostic_codes_in_expected_range" {
    root.compiler.AssertHasCode(
        {
            "missing_method.baml": "interface I { function f(self) -> string } class C { implements I {} }",
        },
        "E0113",
    );
    root.compiler.AssertHasCode(
        { "missing_interface.baml": "class C { implements Missing {} }" },
        "E0002",
    );
    root.compiler.AssertHasCode(
        { "class_target.baml": "class X { x: int } class C { implements X {} }" },
        "E0119",
    )
}

test "interface_default_method_body_gets_exhaustiveness_checking" {
    root.compiler.AssertHasCode(
        {
            "interface_default_method_body_gets_exhaustiveness_checking.baml": "\n        interface Labels {\n            function label(self, value: bool) -> string throws never {\n                return match (value) {\n                    true => \"yes\"\n                }\n            }\n        }\n\n        class Thing {\n            implements Labels {}\n        }\n        ",
        },
        "E0062",
    )
}

test "match_without_catchall_on_interface_is_compile_error" {
    root.compiler.AssertHasCode(
        {
            "match_without_catchall_on_interface_is_compile_error.baml": "\n        interface Animal {\n            function speak(self) -> string throws never\n        }\n        class Dog {\n            implements Animal {\n                function speak(self) -> string { return \"Woof!\" }\n            }\n        }\n        class Cat {\n            implements Animal {\n                function speak(self) -> string { return \"Meow.\" }\n            }\n        }\n\n        function describe(a: Animal) -> string {\n            return match (a) {\n                let d: Dog => \"dog\"\n                let c: Cat => \"cat\"\n            }\n        }\n        ",
        },
        "E0062",
    )
}

test "out_of_body_implements_for_unknown_target_is_error" {
    root.compiler.AssertHasMessage(
        {
            "out_of_body_implements_for_unknown_target_is_error.baml": "\n        interface ToJson {\n            function to_json(self) -> string throws never\n        }\n        implements ToJson for Nonexistent {\n            function to_json(self) -> string { return \"?\" }\n        }\n        ",
        },
        "Nonexistent",
    )
}

test "unresolved_for_target_reported_even_when_interface_unresolved" {
    root.compiler.AssertHasMessage(
        {
            "unresolved_for_target_reported_even_when_interface_unresolved.baml": "\n        implement BadInterface for AlsoMissing {}\n        ",
        },
        "AlsoMissing",
    )
}

test "stolen_turbofish_on_generic_class_impl_static_is_diagnosed_not_ice" {
    root.compiler.AssertHasMessage(
        {
            "stolen_turbofish_on_generic_class_impl_static_is_diagnosed_not_ice.baml": "\n        interface Picker {\n            function pick<X>() -> int throws never\n        }\n        class PickBin<T> {\n            value: T\n            implements Picker {\n                function pick<X>() -> int { return 7 }\n            }\n        }\n        function main() -> int {\n            return PickBin.pick<int>()\n        }\n        ",
        },
        "cannot infer type parameter",
    )
}

test "unqualified_call_on_ambiguous_class_errors" {
    AssertHasDiagnostic(
        {
            "unqualified_call_on_ambiguous_class_errors.baml": "\n        interface Serializer {\n            function encode(self) -> string throws never\n        }\n        interface BinarySerializer {\n            function encode(self) -> string throws never\n        }\n        class Hybrid {\n            implements Serializer {\n                function encode(self) -> string { return \"json\" }\n            }\n            implements BinarySerializer {\n                function encode(self) -> string { return \"bin\" }\n            }\n        }\n        function main() -> string {\n            let h = Hybrid {}\n            return h.encode()\n        }\n        ",
        },
        "E0121",
        ["`Serializer`", "`BinarySerializer`"],
    )
}

test "three_way_unqualified_call_lists_every_source" {
    AssertHasDiagnostic(
        {
            "three_way_unqualified_call_lists_every_source.baml": "\n        interface A {\n            function id(self) -> string throws never\n        }\n        interface B {\n            function id(self) -> string throws never\n        }\n        interface C {\n            function id(self) -> string throws never\n        }\n\n        class Tri {\n            implements A {\n                function id(self) -> string { return \"a\" }\n            }\n            implements B {\n                function id(self) -> string { return \"b\" }\n            }\n            implements C {\n                function id(self) -> string { return \"c\" }\n            }\n        }\n        function main() -> string {\n            let t = Tri {}\n            return t.id()\n        }\n        ",
        },
        "E0121",
        ["`A`", "`B`", "`C`"],
    )
}

test "bounded_blanket_impl_does_not_discharge_against_unrelated_env_param" {
    AssertHasDiagnostic(
        {
            "bounded_blanket_impl_does_not_discharge_against_unrelated_env_param.baml": "\n        interface ProbeMarker {\n            function probe_tag(self) -> int throws never\n        }\n        class ProbeBin<T> {\n            value: T[]\n        }\n        implements<T extends ProbeMarker> ProbeMarker for ProbeBin<T> {\n            function probe_tag(self) -> int { return 1 }\n        }\n        function probe_caller<T extends ProbeMarker>(x: T) -> int {\n            let items = [];\n            let b = ProbeBin { value: items };\n            let n = b.probe_tag();\n            items.push(3);\n            return n\n        }\n        function main() -> int {\n            return 0\n        }\n        ",
        },
        "E0001",
        ["expected `ProbeMarker`, found `int`"],
    )
}

test "union_fuzz_f04_string_plus_int_is_type_error_not_vm_crash" {
    AssertHasDiagnostic(
        {
            "union_fuzz_f04_string_plus_int_is_type_error_not_vm_crash.baml": "\n        function plainPlus() -> string {\n          return \"age=\" + 4\n        }\n        function main() -> string {\n          return plainPlus()\n        }\n        ",
        },
        "E0004",
        ["operator `+`"],
    )
}

test "interface_destructure_before_concrete_destructure_is_unreachable_not_nonexhaustive" {
    root.compiler.AssertHasCode(
        {
            "interface_destructure_before_concrete_destructure_is_unreachable_not_nonexhaustive.baml": "\n        interface Animal {\n            name: string\n        }\n        class Dog {\n            name: string\n            breed: string\n            implements Animal {}\n        }\n\n        function describe(a: Animal) -> string {\n            return match (a) {\n                Animal { name: string } => \"animal\"\n                Dog { breed: \"Lab\" } => \"lab\"\n            }\n        }\n        ",
        },
        "E0063",
    );
    AssertLacksCode(
        {
            "interface_destructure_before_concrete_destructure_is_unreachable_not_nonexhaustive.baml": "\n        interface Animal {\n            name: string\n        }\n        class Dog {\n            name: string\n            breed: string\n            implements Animal {}\n        }\n\n        function describe(a: Animal) -> string {\n            return match (a) {\n                Animal { name: string } => \"animal\"\n                Dog { breed: \"Lab\" } => \"lab\"\n            }\n        }\n        ",
        },
        "E0062",
    )
}

test "required_parent_lookup_uses_declaring_interface_namespace" {
    root.compiler.AssertHasCode(
        {
            "main.baml": "\n                interface Parent {}\n\n                class Robot {\n                    function label(self) -> string { return \"robot\" }\n                    implements root.contracts.Child {}\n                    implements Parent {}\n                }\n                ",
            "ns_contracts/interfaces.baml": "\n                interface Parent {\n                    function label(self) -> string throws never\n                }\n                interface Child requires Parent {}\n                ",
        },
        "E0125",
    )
}

test "out_of_body_empty_implements_for_generic_class_does_not_satisfy_via_inherent_method" {
    root.compiler.AssertHasCode(
        {
            "main.baml": "\n                interface Printable {\n                    function label(self) -> string throws never\n                }\n\n                implements Printable for root.models.Box<int> {}\n                ",
            "ns_models/box.baml": "\n                class Box<T> {\n                    value: T\n\n                    function label(self) -> string {\n                        return \"box\"\n                    }\n                }\n                ",
        },
        "E0113",
    )
}

test "unified_rule_namespaced_classes_with_same_short_name_do_not_cross_match" {
    root.compiler.AssertHasMessage(
        {
            "main.baml": "\n                function take(item: root.a.Printable) -> string {\n                    return item.display()\n                }\n                function bad() -> string {\n                    return take(root.b.Wrapper { value: 42 })\n                }\n                ",
            "ns_a/wrapper.baml": "\n                interface Printable {\n                    function display(self) -> string throws never\n                }\n                class Wrapper {\n                    value: int\n                }\n                implements Printable for Wrapper {\n                    function display(self) -> string { return \"a\" }\n                }\n                ",
            "ns_b/wrapper.baml": "\n                class Wrapper {\n                    value: int\n                }\n                ",
        },
        "b.Wrapper",
    )
}

test "unified_rule_namespaced_generic_classes_with_same_short_name_do_not_cross_match" {
    root.compiler.AssertHasMessage(
        {
            "main.baml": "\n                function take(item: root.a.Printable<int>) -> string {\n                    return item.display()\n                }\n                function bad(item: root.b.Wrapper<int>) -> string {\n                    return take(item)\n                }\n                ",
            "ns_a/wrapper.baml": "\n                interface Printable<T> {\n                    function display(self) -> string throws never\n                }\n                class Wrapper<T> {\n                    value: T\n                }\n                implements<T> Printable<T> for Wrapper<T> {\n                    function display(self) -> string { return \"a\" }\n                }\n                ",
            "ns_b/wrapper.baml": "\n                class Wrapper<T> {\n                    value: T\n                }\n                ",
        },
        "b.Wrapper<int>",
    )
}

test "namespaced_class_cannot_use_unrooted_cross_namespace_qualification" {
    root.compiler.AssertHasMessage(
        {
            "ns_a/a.baml": "\n                interface Named {\n                    function name(self) -> string throws never\n                }\n                ",
            "ns_pets/pets.baml": "\n                class Dog {\n                    implements a.Named {\n                        function name(self) -> string { return \"Rex\" }\n                    }\n                }\n                ",
        },
        "unresolved type: a.Named",
    )
}

test "qualified_generic_constructor_preserves_concrete_type_in_diagnostics" {
    root.compiler.AssertHasMessage(
        {
            "main.baml": "\n                interface Printable<T> {\n                    function display(self) -> string throws never\n                }\n                class Box<T> {\n                    value: T\n                }\n                function take(item: Printable<int>) -> string {\n                    return item.display()\n                }\n                function bad() -> string {\n                    return take(root.b.Box<int> { value: 42 })\n                }\n                ",
            "ns_b/box.baml": "\n                class Box<T> {\n                    value: T\n                }\n                ",
        },
        "b.Box<int>",
    )
}

test "wf3_ambiguous_method_suggestion_resolvable_inside_namespace" {
    root.compiler.AssertHasMessage(
        {
            "main.baml": "function main() -> string { let p = birds.Parrot {} return p.run() }",
            "ns_birds/birds.baml": "\n            class Parrot {\n                implements root.zoo.Animal { function speak(self) -> string { return \"squawk\" } }\n                implements root.farm.Animal { function speak(self) -> string { return \"cluck\" } }\n                function run(self) -> string { return self.speak() }\n            }\n            ",
            "ns_zoo/zoo.baml": "interface Animal { function speak(self) -> string throws never }",
            "ns_farm/farm.baml": "interface Animal { function speak(self) -> string throws never }",
        },
        "root.zoo.Animal",
    )
}

test "wf3_bare_cross_ns_requires_unknown_echoes_qualifier" {
    root.compiler.AssertHasMessage(
        {
            "main.baml": "function main() -> string { return \"ok\" }",
            "ns_a/a.baml": "interface Base { function f(self) -> string throws never }",
            "ns_b/b.baml": "interface Derived requires root.a.Ghost {}",
        },
        "root.a.Ghost",
    )
}

test "selfless_interface_method_is_not_reachable_through_a_value" {
    AssertHasDiagnostic(
        {
            "selfless_interface_method_is_not_reachable_through_a_value_0.baml": "\n        interface Buildable {\n            function build(seed: int) -> Self\n        }\n        class Gadget {\n            implements Buildable {\n                function build(seed: int) -> Self throws never { Gadget {} }\n            }\n        }\n        function main() -> void {\n            let g = Gadget {}\n            g.build(1)\n        }\n        ",
        },
        "E0001",
        ["on interface `Buildable`"],
    );
    AssertHasDiagnostic(
        {
            "selfless_interface_method_is_not_reachable_through_a_value_1.baml": "\n        interface Buildable {\n            function build(seed: int) -> Self\n        }\n        class Gadget {\n            implements Buildable {\n                function build(seed: int) -> Self throws never { Gadget {} }\n            }\n        }\n        function main() -> void {\n            let g = Gadget {}\n            let f = g.build\n        }\n        ",
        },
        "E0001",
        ["on interface `Buildable`"],
    )
}

test "selfless_inherent_method_is_not_reachable_through_a_value" {
    AssertHasDiagnostic(
        {
            "selfless_inherent_method_is_not_reachable_through_a_value_0.baml": "\n        class Widget {\n            n: int\n            function make(seed: int) -> Widget throws never { Widget { n: seed } }\n        }\n        function main() -> void {\n            let w = Widget { n: 0 }\n            w.make(1)\n        }\n        ",
        },
        "E0001",
        ["`TypeName.make(...)`"],
    );
    AssertHasDiagnostic(
        {
            "selfless_inherent_method_is_not_reachable_through_a_value_1.baml": "\n        class Widget {\n            n: int\n            function make(seed: int) -> Widget throws never { Widget { n: seed } }\n        }\n        function main() -> void {\n            let w = Widget { n: 0 }\n            let f = w.make\n        }\n        ",
        },
        "E0001",
        ["`TypeName.make(...)`"],
    )
}

test "form1_bounded_generic_receiver_rejects_when_bound_not_satisfied" {
    root.compiler.AssertHasMessage(
        {
            "form1_bounded_generic_receiver_rejects_when_bound_not_satisfied.baml": "\n    interface Named {\n        name: string\n    }\n    interface Printable {\n        function display(self) -> string throws never\n    }\n    class Rock {\n        label: string\n    }\n    class Wrapper<T> {\n        inner: T\n    }\n    implements<T extends Named> Printable for Wrapper<T> {\n        function display(self) -> string { return self.inner.name }\n    }\n    function bad() -> string {\n        let item: Printable = Wrapper<Rock> { inner: Rock { label: \"igneous\" } }\n        return item.display()\n    }\n    ",
        },
        "Wrapper",
    );
    root.compiler.AssertHasMessage(
        {
            "form1_bounded_generic_receiver_rejects_when_bound_not_satisfied.baml": "\n    interface Named {\n        name: string\n    }\n    interface Printable {\n        function display(self) -> string throws never\n    }\n    class Rock {\n        label: string\n    }\n    class Wrapper<T> {\n        inner: T\n    }\n    implements<T extends Named> Printable for Wrapper<T> {\n        function display(self) -> string { return self.inner.name }\n    }\n    function bad() -> string {\n        let item: Printable = Wrapper<Rock> { inner: Rock { label: \"igneous\" } }\n        return item.display()\n    }\n    ",
        },
        "Rock",
    )
}

test "malformed_impls_do_not_spuriously_overlap" {
    AssertCodeCount(
        {
            "malformed_impls_do_not_spuriously_overlap.baml": "\n        interface Marker {}\n        implement Marker for Nonexistent1 {}\n        implement Marker for Nonexistent2 {}\n        ",
        },
        "E0002",
        2,
    );
    AssertLacksCode(
        {
            "malformed_impls_do_not_spuriously_overlap.baml": "\n        interface Marker {}\n        implement Marker for Nonexistent1 {}\n        implement Marker for Nonexistent2 {}\n        ",
        },
        "E0132",
    )
}

test "implements_target_arg_error_is_reported_exactly_once" {
    root.compiler.AssertHasCode(
        {
            "implements_target_arg_error_is_reported_exactly_once_in_body.baml": "\n        interface Container<T> {\n            function size(self) -> int throws never\n        }\n        class Box {\n            items: int[]\n            implements Container<DoesNotExist> {\n                function size(self) -> int { return 0 }\n            }\n        }\n        ",
        },
        "E0002",
    );
    root.compiler.AssertMessageCount(
        {
            "implements_target_arg_error_is_reported_exactly_once_in_body.baml": "\n        interface Container<T> {\n            function size(self) -> int throws never\n        }\n        class Box {\n            items: int[]\n            implements Container<DoesNotExist> {\n                function size(self) -> int { return 0 }\n            }\n        }\n        ",
        },
        "DoesNotExist",
        1,
    );
    root.compiler.AssertMessageCount(
        {
            "implements_target_arg_error_is_reported_exactly_once_out_of_body.baml": "\n        interface Container<T> {\n            function size(self) -> int throws never\n        }\n        class Cat {\n            name: string\n        }\n        implement Container<DoesNotExist> for Cat {\n            function size(self) -> int { return 0 }\n        }\n        ",
        },
        "DoesNotExist",
        1,
    )
}

test "requires_unknown_name_is_unknown_interface_not_non_interface" {
    root.compiler.AssertHasCode(
        {
            "requires_unknown_name_is_unknown_interface_not_non_interface.baml": "\n        interface Person requires DoesNotExist {\n            name: string\n        }\n        ",
        },
        "E0002",
    );
    AssertLacksCode(
        {
            "requires_unknown_name_is_unknown_interface_not_non_interface.baml": "\n        interface Person requires DoesNotExist {\n            name: string\n        }\n        ",
        },
        "E0133",
    )
}

test "ambiguous_method_across_namespaces_uses_qualified_interface_names" {
    AssertHasDiagnostic(
        {
            "main.baml": "\n            class Parrot {\n                implements zoo.Animal {\n                    function speak(self) -> string { return \"squawk\" }\n                }\n                implements farm.Animal {\n                    function speak(self) -> string { return \"cluck\" }\n                }\n            }\n            function main() -> string {\n                let p = Parrot {}\n                return p.speak()\n            }\n            ",
            "ns_zoo/zoo.baml": "interface Animal { function speak(self) -> string throws never }",
            "ns_farm/farm.baml": "interface Animal { function speak(self) -> string throws never }",
        },
        "E0121",
        ["zoo.Animal", "farm.Animal", "as<zoo.Animal>", "as<farm.Animal>"],
    )
}

test "wf3_self_return_wrong_concrete_class_is_rejected" {
    AssertHasErrors(
        {
            "wf3_self_return_wrong_concrete_class_is_rejected.baml": "\n        interface Cloneable {\n            function clone(self) -> Self throws never\n        }\n        class Box {\n            boxField: int\n            implements Cloneable {\n                function clone(self) -> Self {\n                    return Cup { cupField: 99 }\n                }\n            }\n        }\n        class Cup {\n            cupField: int\n            implements Cloneable {\n                function clone(self) -> Self {\n                    return Cup { cupField: self.cupField }\n                }\n            }\n        }\n        function main() -> int {\n            let b = Box { boxField: 1 }\n            let c = b.clone()\n            return c.boxField\n        }\n        ",
        },
    )
}

test "wf3_monomorph_collision_assignment_is_diagnosed" {
    AssertHasErrors(
        {
            "wf3_monomorph_collision_assignment_is_diagnosed.baml": "\n        interface Getter<T> {\n            function get(self) -> T throws never\n        }\n        class Pair<L, R> {\n            left: L\n            right: R\n            implements Getter<L> {\n                function get(self) -> L { return self.left }\n            }\n            implements Getter<R> {\n                function get(self) -> R { return self.right }\n            }\n        }\n        function main() -> int {\n            let p: Pair<int, int> = Pair { left: 7, right: 99 }\n            let g: Getter<int> = p\n            return g.get()\n        }\n        ",
        },
    )
}

test "wf3_self_in_field_type_has_no_contradictory_diagnostics" {
    let advice_is_self_opt = false;
    for (
        let diagnostic in ErrorDiagnostics(
            {
                "wf3_self_in_field_type_has_no_contradictory_diagnostics_concrete.baml": "\n        interface Node {\n            value: int\n            next: Self?\n        }\n        class LinkedItem {\n            value: int\n            next: LinkedItem?\n            implements Node {}\n        }\n        function main() -> int {\n            let tail = LinkedItem { value: 2, next: null }\n            let head = LinkedItem { value: 1, next: tail }\n            return match (head.next) {\n                let n: LinkedItem => n.value,\n                _ => -1,\n            }\n        }\n        ",
            },
        )
    ) {
        if (diagnostic.code == "E0116" && diagnostic.message.includes("Self?")) {
            advice_is_self_opt = true;
        }
    }
    let self_opt_is_unresolved = false;
    for (
        let diagnostic in ErrorDiagnostics(
            {
                "wf3_self_in_field_type_has_no_contradictory_diagnostics_self.baml": "\n        interface Node {\n            value: int\n            next: Self?\n        }\n        class LinkedItem {\n            value: int\n            next: Self?\n            implements Node {}\n        }\n        function main() -> int { return 0 }\n        ",
            },
        )
    ) {
        if (diagnostic.message.includes("unresolved type: Self")) {
            self_opt_is_unresolved = true;
        }
    }
    assert.equal(advice_is_self_opt && self_opt_is_unresolved, false)
}

test "wf3_method_on_interface_union_blames_correct_member" {
    root.compiler.AssertHasMessage(
        {
            "wf3_method_on_interface_union_blames_correct_member.baml": "\n        interface Animal {\n            function speak(self) -> string throws never\n        }\n        interface Swimmer {\n            function swim(self) -> string throws never\n        }\n        class Dog {\n            implements Animal {\n                function speak(self) -> string { return \"Woof!\" }\n            }\n            implements Swimmer {\n                function swim(self) -> string { return \"splash\" }\n            }\n        }\n        function describe(x: Animal | Swimmer) -> string {\n            return x.speak()\n        }\n        function main() -> string {\n            let a: Animal = Dog {}\n            return describe(a)\n        }\n        ",
        },
        "no common interface that declares",
    );
    AssertLacksDiagnostic(
        {
            "wf3_method_on_interface_union_blames_correct_member.baml": "\n        interface Animal {\n            function speak(self) -> string throws never\n        }\n        interface Swimmer {\n            function swim(self) -> string throws never\n        }\n        class Dog {\n            implements Animal {\n                function speak(self) -> string { return \"Woof!\" }\n            }\n            implements Swimmer {\n                function swim(self) -> string { return \"splash\" }\n            }\n        }\n        function describe(x: Animal | Swimmer) -> string {\n            return x.speak()\n        }\n        function main() -> string {\n            let a: Animal = Dog {}\n            return describe(a)\n        }\n        ",
        },
        ["`Animal` has no member"],
    )
}

test "phantom_impl_param_call_is_diagnosed_not_ice" {
    root.compiler.AssertHasCode(
        {
            "phantom_impl_param_call_is_diagnosed_not_ice.baml": "\n        interface PhantomMarker {\n            function tag(self) -> int throws never\n        }\n        class PhantomBin<T> {\n            value: T\n        }\n        implements<T, U> PhantomMarker for PhantomBin<T> {\n            function tag(self) -> int { return 1 }\n        }\n        function use_marker(b: PhantomBin<int>) -> int {\n            return b.tag()\n        }\n        function main() -> int {\n            return use_marker(PhantomBin { value: 3 })\n        }\n        ",
        },
        "E0135",
    )
}

test "union_fuzz_pr_ambiguous_inherited_method_in_class_union_is_e0121" {
    root.compiler.AssertHasCode(
        {
            "union_fuzz_pr_ambiguous_inherited_method_in_class_union_is_e0121.baml": "\n        interface A { function f(self) -> string  throws never { return \"a\" } }\n        interface B { function f(self) -> string  throws never { return \"b\" } }\n        class C { implements A {} implements B {} }\n        class D { implements A {} implements B {} }\n        function g(x: C | D) -> string { return x.f() }\n        function main() -> string { return g(C {}) }\n        ",
        },
        "E0121",
    );
    AssertAllErrorsLack(
        {
            "union_fuzz_pr_ambiguous_inherited_method_in_class_union_is_e0121.baml": "\n        interface A { function f(self) -> string  throws never { return \"a\" } }\n        interface B { function f(self) -> string  throws never { return \"b\" } }\n        class C { implements A {} implements B {} }\n        class D { implements A {} implements B {} }\n        function g(x: C | D) -> string { return x.f() }\n        function main() -> string { return g(C {}) }\n        ",
        },
        ["unknown |", "is not a function"],
    )
}

test "union_fuzz_pr_concrete_arm_not_callable_despite_recovery_arm" {
    root.compiler.AssertHasCode(
        {
            "union_fuzz_pr_concrete_arm_not_callable_despite_recovery_arm.baml": "\n        function f(x: int | DoesNotExist) -> string { return x() }\n        function main() -> string { return \"ok\" }\n        ",
        },
        "E0006",
    )
}

test "union_fuzz_f12_iface_union_call_diagnostic_does_not_leak_internals" {
    AssertAllErrorsLack(
        {
            "union_fuzz_f12_iface_union_call_diagnostic_does_not_leak_internals.baml": "\n        interface Animal {\n          function speak(self) -> string throws never\n        }\n        class Dog {\n          implements Animal {\n            function speak(self) -> string { return \"Woof\" }\n          }\n        }\n        class Cat {\n          implements Animal {\n            function speak(self) -> string { return \"Meow\" }\n          }\n        }\n        function describe(x: Animal | Cat) -> string {\n          return x.speak()\n        }\n        function main() -> string {\n          let a: Animal = Dog {}\n          return describe(a)\n        }\n        ",
        },
        ["throws never", "(self:"],
    )
}

test "union_fuzz_f13_exhaustiveness_witness_does_not_leak_user_prefix" {
    root.compiler.AssertHasCode(
        {
            "union_fuzz_f13_exhaustiveness_witness_does_not_leak_user_prefix.baml": "\n        class Person { name: string }\n        class Dog { owner: Person }\n        class Cat { lives: int }\n        function describe(x: Dog | Cat) -> string {\n          match (x) {\n            Dog { owner: Person { name: \"Alice\" } } => \"alice's dog\"\n          }\n        }\n        function main() -> string {\n          let p: Person = Person { name: \"Bob\" }\n          let d: Dog = Dog { owner: p }\n          return describe(d)\n        }\n        ",
        },
        "E0062",
    );
    AssertAllErrorsLack(
        {
            "union_fuzz_f13_exhaustiveness_witness_does_not_leak_user_prefix.baml": "\n        class Person { name: string }\n        class Dog { owner: Person }\n        class Cat { lives: int }\n        function describe(x: Dog | Cat) -> string {\n          match (x) {\n            Dog { owner: Person { name: \"Alice\" } } => \"alice's dog\"\n          }\n        }\n        function main() -> string {\n          let p: Person = Person { name: \"Bob\" }\n          let d: Dog = Dog { owner: p }\n          return describe(d)\n        }\n        ",
        },
        ["user."],
    )
}

test "union_fuzz_f14_exhaustiveness_witness_names_interface_members" {
    let found = false;
    for (
        let diagnostic in ErrorDiagnostics(
            {
                "union_fuzz_f14_exhaustiveness_witness_names_interface_members.baml": "\n        interface Animal { function speak(self) -> string throws never }\n        interface Vehicle { function drive(self) -> string throws never }\n        class Dog { name: string  implements Animal { function speak(self) -> string { return \"Woof\" } } }\n        class Car { model: string  implements Vehicle { function drive(self) -> string { return \"Vroom\" } } }\n        function describe(x: Animal | Vehicle) -> string {\n          match (x) {\n            Dog { name } => \"dog \" + name\n            Car { model } => \"car \" + model\n          }\n        }\n        function main() -> string {\n          let c: Car = Car { model: \"T\" }\n          return describe(c)\n        }\n        ",
            },
        )
    ) {
        if (
            diagnostic.code == "E0062"
                && diagnostic.message.includes("missing:")
                && (diagnostic.message.includes("Animal") || diagnostic.message.includes("Vehicle"))
        ) {
            found = true;
        }
    }
    assert.equal(found, true)
}

test "union_fuzz_f15_union_method_blame_skips_satisfying_member" {
    AssertHasDiagnostic(
        {
            "union_fuzz_f15_union_method_blame_skips_satisfying_member.baml": "\n        interface Debuggable { function debug(self) -> string throws never }\n        implements Debuggable for int { function debug(self) -> string { return \"int\" } }\n        class Dog { name: string }\n        function main() -> string {\n          let x: int | Dog = 7\n          return x.debug()\n        }\n        ",
        },
        "",
        ["Dog", "debug"],
    );
    AssertLacksDiagnostic(
        {
            "union_fuzz_f15_union_method_blame_skips_satisfying_member.baml": "\n        interface Debuggable { function debug(self) -> string throws never }\n        implements Debuggable for int { function debug(self) -> string { return \"int\" } }\n        class Dog { name: string }\n        function main() -> string {\n          let x: int | Dog = 7\n          return x.debug()\n        }\n        ",
        },
        ["`int`", "debug"],
    )
}

test "scalar_arithmetic_matches_builtin_impl_matrix" {
    root.compiler.AssertCompiles(
        {
            "scalar_arithmetic_matches_builtin_impl_matrix_accepted.baml": "function add_int_int(lhs: int, rhs: int) -> int { lhs + rhs; 0 }\nfunction sub_int_int(lhs: int, rhs: int) -> int { lhs - rhs; 0 }\nfunction mul_int_int(lhs: int, rhs: int) -> int { lhs * rhs; 0 }\nfunction div_int_int(lhs: int, rhs: int) -> int { lhs / rhs; 0 }\nfunction rem_int_int(lhs: int, rhs: int) -> int { lhs % rhs; 0 }\nfunction add_int_float(lhs: int, rhs: float) -> int { lhs + rhs; 0 }\nfunction sub_int_float(lhs: int, rhs: float) -> int { lhs - rhs; 0 }\nfunction mul_int_float(lhs: int, rhs: float) -> int { lhs * rhs; 0 }\nfunction div_int_float(lhs: int, rhs: float) -> int { lhs / rhs; 0 }\nfunction rem_int_float(lhs: int, rhs: float) -> int { lhs % rhs; 0 }\nfunction add_int_bigint(lhs: int, rhs: bigint) -> int { lhs + rhs; 0 }\nfunction sub_int_bigint(lhs: int, rhs: bigint) -> int { lhs - rhs; 0 }\nfunction mul_int_bigint(lhs: int, rhs: bigint) -> int { lhs * rhs; 0 }\nfunction div_int_bigint(lhs: int, rhs: bigint) -> int { lhs / rhs; 0 }\nfunction rem_int_bigint(lhs: int, rhs: bigint) -> int { lhs % rhs; 0 }\nfunction add_float_int(lhs: float, rhs: int) -> int { lhs + rhs; 0 }\nfunction sub_float_int(lhs: float, rhs: int) -> int { lhs - rhs; 0 }\nfunction mul_float_int(lhs: float, rhs: int) -> int { lhs * rhs; 0 }\nfunction div_float_int(lhs: float, rhs: int) -> int { lhs / rhs; 0 }\nfunction rem_float_int(lhs: float, rhs: int) -> int { lhs % rhs; 0 }\nfunction add_float_float(lhs: float, rhs: float) -> int { lhs + rhs; 0 }\nfunction sub_float_float(lhs: float, rhs: float) -> int { lhs - rhs; 0 }\nfunction mul_float_float(lhs: float, rhs: float) -> int { lhs * rhs; 0 }\nfunction div_float_float(lhs: float, rhs: float) -> int { lhs / rhs; 0 }\nfunction rem_float_float(lhs: float, rhs: float) -> int { lhs % rhs; 0 }\nfunction add_bigint_int(lhs: bigint, rhs: int) -> int { lhs + rhs; 0 }\nfunction sub_bigint_int(lhs: bigint, rhs: int) -> int { lhs - rhs; 0 }\nfunction mul_bigint_int(lhs: bigint, rhs: int) -> int { lhs * rhs; 0 }\nfunction div_bigint_int(lhs: bigint, rhs: int) -> int { lhs / rhs; 0 }\nfunction rem_bigint_int(lhs: bigint, rhs: int) -> int { lhs % rhs; 0 }\nfunction add_bigint_bigint(lhs: bigint, rhs: bigint) -> int { lhs + rhs; 0 }\nfunction sub_bigint_bigint(lhs: bigint, rhs: bigint) -> int { lhs - rhs; 0 }\nfunction mul_bigint_bigint(lhs: bigint, rhs: bigint) -> int { lhs * rhs; 0 }\nfunction div_bigint_bigint(lhs: bigint, rhs: bigint) -> int { lhs / rhs; 0 }\nfunction rem_bigint_bigint(lhs: bigint, rhs: bigint) -> int { lhs % rhs; 0 }\nfunction add_string_string(lhs: string, rhs: string) -> int { lhs + rhs; 0 }\n",
        },
    );
    AssertErrorCount(
        {
            "scalar_arithmetic_matches_builtin_impl_matrix_rejected.baml": "function add_int_string(lhs: int, rhs: string) -> int { lhs + rhs; 0 }\nfunction sub_int_string(lhs: int, rhs: string) -> int { lhs - rhs; 0 }\nfunction mul_int_string(lhs: int, rhs: string) -> int { lhs * rhs; 0 }\nfunction div_int_string(lhs: int, rhs: string) -> int { lhs / rhs; 0 }\nfunction rem_int_string(lhs: int, rhs: string) -> int { lhs % rhs; 0 }\nfunction add_int_bool(lhs: int, rhs: bool) -> int { lhs + rhs; 0 }\nfunction sub_int_bool(lhs: int, rhs: bool) -> int { lhs - rhs; 0 }\nfunction mul_int_bool(lhs: int, rhs: bool) -> int { lhs * rhs; 0 }\nfunction div_int_bool(lhs: int, rhs: bool) -> int { lhs / rhs; 0 }\nfunction rem_int_bool(lhs: int, rhs: bool) -> int { lhs % rhs; 0 }\nfunction add_int_null(lhs: int, rhs: null) -> int { lhs + rhs; 0 }\nfunction sub_int_null(lhs: int, rhs: null) -> int { lhs - rhs; 0 }\nfunction mul_int_null(lhs: int, rhs: null) -> int { lhs * rhs; 0 }\nfunction div_int_null(lhs: int, rhs: null) -> int { lhs / rhs; 0 }\nfunction rem_int_null(lhs: int, rhs: null) -> int { lhs % rhs; 0 }\nfunction add_float_bigint(lhs: float, rhs: bigint) -> int { lhs + rhs; 0 }\nfunction sub_float_bigint(lhs: float, rhs: bigint) -> int { lhs - rhs; 0 }\nfunction mul_float_bigint(lhs: float, rhs: bigint) -> int { lhs * rhs; 0 }\nfunction div_float_bigint(lhs: float, rhs: bigint) -> int { lhs / rhs; 0 }\nfunction rem_float_bigint(lhs: float, rhs: bigint) -> int { lhs % rhs; 0 }\nfunction add_float_string(lhs: float, rhs: string) -> int { lhs + rhs; 0 }\nfunction sub_float_string(lhs: float, rhs: string) -> int { lhs - rhs; 0 }\nfunction mul_float_string(lhs: float, rhs: string) -> int { lhs * rhs; 0 }\nfunction div_float_string(lhs: float, rhs: string) -> int { lhs / rhs; 0 }\nfunction rem_float_string(lhs: float, rhs: string) -> int { lhs % rhs; 0 }\nfunction add_float_bool(lhs: float, rhs: bool) -> int { lhs + rhs; 0 }\nfunction sub_float_bool(lhs: float, rhs: bool) -> int { lhs - rhs; 0 }\nfunction mul_float_bool(lhs: float, rhs: bool) -> int { lhs * rhs; 0 }\nfunction div_float_bool(lhs: float, rhs: bool) -> int { lhs / rhs; 0 }\nfunction rem_float_bool(lhs: float, rhs: bool) -> int { lhs % rhs; 0 }\nfunction add_float_null(lhs: float, rhs: null) -> int { lhs + rhs; 0 }\nfunction sub_float_null(lhs: float, rhs: null) -> int { lhs - rhs; 0 }\nfunction mul_float_null(lhs: float, rhs: null) -> int { lhs * rhs; 0 }\nfunction div_float_null(lhs: float, rhs: null) -> int { lhs / rhs; 0 }\nfunction rem_float_null(lhs: float, rhs: null) -> int { lhs % rhs; 0 }\nfunction add_bigint_float(lhs: bigint, rhs: float) -> int { lhs + rhs; 0 }\nfunction sub_bigint_float(lhs: bigint, rhs: float) -> int { lhs - rhs; 0 }\nfunction mul_bigint_float(lhs: bigint, rhs: float) -> int { lhs * rhs; 0 }\nfunction div_bigint_float(lhs: bigint, rhs: float) -> int { lhs / rhs; 0 }\nfunction rem_bigint_float(lhs: bigint, rhs: float) -> int { lhs % rhs; 0 }\nfunction add_bigint_string(lhs: bigint, rhs: string) -> int { lhs + rhs; 0 }\nfunction sub_bigint_string(lhs: bigint, rhs: string) -> int { lhs - rhs; 0 }\nfunction mul_bigint_string(lhs: bigint, rhs: string) -> int { lhs * rhs; 0 }\nfunction div_bigint_string(lhs: bigint, rhs: string) -> int { lhs / rhs; 0 }\nfunction rem_bigint_string(lhs: bigint, rhs: string) -> int { lhs % rhs; 0 }\nfunction add_bigint_bool(lhs: bigint, rhs: bool) -> int { lhs + rhs; 0 }\nfunction sub_bigint_bool(lhs: bigint, rhs: bool) -> int { lhs - rhs; 0 }\nfunction mul_bigint_bool(lhs: bigint, rhs: bool) -> int { lhs * rhs; 0 }\nfunction div_bigint_bool(lhs: bigint, rhs: bool) -> int { lhs / rhs; 0 }\nfunction rem_bigint_bool(lhs: bigint, rhs: bool) -> int { lhs % rhs; 0 }\nfunction add_bigint_null(lhs: bigint, rhs: null) -> int { lhs + rhs; 0 }\nfunction sub_bigint_null(lhs: bigint, rhs: null) -> int { lhs - rhs; 0 }\nfunction mul_bigint_null(lhs: bigint, rhs: null) -> int { lhs * rhs; 0 }\nfunction div_bigint_null(lhs: bigint, rhs: null) -> int { lhs / rhs; 0 }\nfunction rem_bigint_null(lhs: bigint, rhs: null) -> int { lhs % rhs; 0 }\nfunction add_string_int(lhs: string, rhs: int) -> int { lhs + rhs; 0 }\nfunction sub_string_int(lhs: string, rhs: int) -> int { lhs - rhs; 0 }\nfunction mul_string_int(lhs: string, rhs: int) -> int { lhs * rhs; 0 }\nfunction div_string_int(lhs: string, rhs: int) -> int { lhs / rhs; 0 }\nfunction rem_string_int(lhs: string, rhs: int) -> int { lhs % rhs; 0 }\nfunction add_string_float(lhs: string, rhs: float) -> int { lhs + rhs; 0 }\nfunction sub_string_float(lhs: string, rhs: float) -> int { lhs - rhs; 0 }\nfunction mul_string_float(lhs: string, rhs: float) -> int { lhs * rhs; 0 }\nfunction div_string_float(lhs: string, rhs: float) -> int { lhs / rhs; 0 }\nfunction rem_string_float(lhs: string, rhs: float) -> int { lhs % rhs; 0 }\nfunction add_string_bigint(lhs: string, rhs: bigint) -> int { lhs + rhs; 0 }\nfunction sub_string_bigint(lhs: string, rhs: bigint) -> int { lhs - rhs; 0 }\nfunction mul_string_bigint(lhs: string, rhs: bigint) -> int { lhs * rhs; 0 }\nfunction div_string_bigint(lhs: string, rhs: bigint) -> int { lhs / rhs; 0 }\nfunction rem_string_bigint(lhs: string, rhs: bigint) -> int { lhs % rhs; 0 }\nfunction sub_string_string(lhs: string, rhs: string) -> int { lhs - rhs; 0 }\nfunction mul_string_string(lhs: string, rhs: string) -> int { lhs * rhs; 0 }\nfunction div_string_string(lhs: string, rhs: string) -> int { lhs / rhs; 0 }\nfunction rem_string_string(lhs: string, rhs: string) -> int { lhs % rhs; 0 }\nfunction add_string_bool(lhs: string, rhs: bool) -> int { lhs + rhs; 0 }\nfunction sub_string_bool(lhs: string, rhs: bool) -> int { lhs - rhs; 0 }\nfunction mul_string_bool(lhs: string, rhs: bool) -> int { lhs * rhs; 0 }\nfunction div_string_bool(lhs: string, rhs: bool) -> int { lhs / rhs; 0 }\nfunction rem_string_bool(lhs: string, rhs: bool) -> int { lhs % rhs; 0 }\nfunction add_string_null(lhs: string, rhs: null) -> int { lhs + rhs; 0 }\nfunction sub_string_null(lhs: string, rhs: null) -> int { lhs - rhs; 0 }\nfunction mul_string_null(lhs: string, rhs: null) -> int { lhs * rhs; 0 }\nfunction div_string_null(lhs: string, rhs: null) -> int { lhs / rhs; 0 }\nfunction rem_string_null(lhs: string, rhs: null) -> int { lhs % rhs; 0 }\nfunction add_bool_int(lhs: bool, rhs: int) -> int { lhs + rhs; 0 }\nfunction sub_bool_int(lhs: bool, rhs: int) -> int { lhs - rhs; 0 }\nfunction mul_bool_int(lhs: bool, rhs: int) -> int { lhs * rhs; 0 }\nfunction div_bool_int(lhs: bool, rhs: int) -> int { lhs / rhs; 0 }\nfunction rem_bool_int(lhs: bool, rhs: int) -> int { lhs % rhs; 0 }\nfunction add_bool_float(lhs: bool, rhs: float) -> int { lhs + rhs; 0 }\nfunction sub_bool_float(lhs: bool, rhs: float) -> int { lhs - rhs; 0 }\nfunction mul_bool_float(lhs: bool, rhs: float) -> int { lhs * rhs; 0 }\nfunction div_bool_float(lhs: bool, rhs: float) -> int { lhs / rhs; 0 }\nfunction rem_bool_float(lhs: bool, rhs: float) -> int { lhs % rhs; 0 }\nfunction add_bool_bigint(lhs: bool, rhs: bigint) -> int { lhs + rhs; 0 }\nfunction sub_bool_bigint(lhs: bool, rhs: bigint) -> int { lhs - rhs; 0 }\nfunction mul_bool_bigint(lhs: bool, rhs: bigint) -> int { lhs * rhs; 0 }\nfunction div_bool_bigint(lhs: bool, rhs: bigint) -> int { lhs / rhs; 0 }\nfunction rem_bool_bigint(lhs: bool, rhs: bigint) -> int { lhs % rhs; 0 }\nfunction add_bool_string(lhs: bool, rhs: string) -> int { lhs + rhs; 0 }\nfunction sub_bool_string(lhs: bool, rhs: string) -> int { lhs - rhs; 0 }\nfunction mul_bool_string(lhs: bool, rhs: string) -> int { lhs * rhs; 0 }\nfunction div_bool_string(lhs: bool, rhs: string) -> int { lhs / rhs; 0 }\nfunction rem_bool_string(lhs: bool, rhs: string) -> int { lhs % rhs; 0 }\nfunction add_bool_bool(lhs: bool, rhs: bool) -> int { lhs + rhs; 0 }\nfunction sub_bool_bool(lhs: bool, rhs: bool) -> int { lhs - rhs; 0 }\nfunction mul_bool_bool(lhs: bool, rhs: bool) -> int { lhs * rhs; 0 }\nfunction div_bool_bool(lhs: bool, rhs: bool) -> int { lhs / rhs; 0 }\nfunction rem_bool_bool(lhs: bool, rhs: bool) -> int { lhs % rhs; 0 }\nfunction add_bool_null(lhs: bool, rhs: null) -> int { lhs + rhs; 0 }\nfunction sub_bool_null(lhs: bool, rhs: null) -> int { lhs - rhs; 0 }\nfunction mul_bool_null(lhs: bool, rhs: null) -> int { lhs * rhs; 0 }\nfunction div_bool_null(lhs: bool, rhs: null) -> int { lhs / rhs; 0 }\nfunction rem_bool_null(lhs: bool, rhs: null) -> int { lhs % rhs; 0 }\nfunction add_null_int(lhs: null, rhs: int) -> int { lhs + rhs; 0 }\nfunction sub_null_int(lhs: null, rhs: int) -> int { lhs - rhs; 0 }\nfunction mul_null_int(lhs: null, rhs: int) -> int { lhs * rhs; 0 }\nfunction div_null_int(lhs: null, rhs: int) -> int { lhs / rhs; 0 }\nfunction rem_null_int(lhs: null, rhs: int) -> int { lhs % rhs; 0 }\nfunction add_null_float(lhs: null, rhs: float) -> int { lhs + rhs; 0 }\nfunction sub_null_float(lhs: null, rhs: float) -> int { lhs - rhs; 0 }\nfunction mul_null_float(lhs: null, rhs: float) -> int { lhs * rhs; 0 }\nfunction div_null_float(lhs: null, rhs: float) -> int { lhs / rhs; 0 }\nfunction rem_null_float(lhs: null, rhs: float) -> int { lhs % rhs; 0 }\nfunction add_null_bigint(lhs: null, rhs: bigint) -> int { lhs + rhs; 0 }\nfunction sub_null_bigint(lhs: null, rhs: bigint) -> int { lhs - rhs; 0 }\nfunction mul_null_bigint(lhs: null, rhs: bigint) -> int { lhs * rhs; 0 }\nfunction div_null_bigint(lhs: null, rhs: bigint) -> int { lhs / rhs; 0 }\nfunction rem_null_bigint(lhs: null, rhs: bigint) -> int { lhs % rhs; 0 }\nfunction add_null_string(lhs: null, rhs: string) -> int { lhs + rhs; 0 }\nfunction sub_null_string(lhs: null, rhs: string) -> int { lhs - rhs; 0 }\nfunction mul_null_string(lhs: null, rhs: string) -> int { lhs * rhs; 0 }\nfunction div_null_string(lhs: null, rhs: string) -> int { lhs / rhs; 0 }\nfunction rem_null_string(lhs: null, rhs: string) -> int { lhs % rhs; 0 }\nfunction add_null_bool(lhs: null, rhs: bool) -> int { lhs + rhs; 0 }\nfunction sub_null_bool(lhs: null, rhs: bool) -> int { lhs - rhs; 0 }\nfunction mul_null_bool(lhs: null, rhs: bool) -> int { lhs * rhs; 0 }\nfunction div_null_bool(lhs: null, rhs: bool) -> int { lhs / rhs; 0 }\nfunction rem_null_bool(lhs: null, rhs: bool) -> int { lhs % rhs; 0 }\nfunction add_null_null(lhs: null, rhs: null) -> int { lhs + rhs; 0 }\nfunction sub_null_null(lhs: null, rhs: null) -> int { lhs - rhs; 0 }\nfunction mul_null_null(lhs: null, rhs: null) -> int { lhs * rhs; 0 }\nfunction div_null_null(lhs: null, rhs: null) -> int { lhs / rhs; 0 }\nfunction rem_null_null(lhs: null, rhs: null) -> int { lhs % rhs; 0 }\n",
        },
        144,
    );
    AssertAllErrorsContain(
        {
            "scalar_arithmetic_matches_builtin_impl_matrix_rejected.baml": "function add_int_string(lhs: int, rhs: string) -> int { lhs + rhs; 0 }\nfunction sub_int_string(lhs: int, rhs: string) -> int { lhs - rhs; 0 }\nfunction mul_int_string(lhs: int, rhs: string) -> int { lhs * rhs; 0 }\nfunction div_int_string(lhs: int, rhs: string) -> int { lhs / rhs; 0 }\nfunction rem_int_string(lhs: int, rhs: string) -> int { lhs % rhs; 0 }\nfunction add_int_bool(lhs: int, rhs: bool) -> int { lhs + rhs; 0 }\nfunction sub_int_bool(lhs: int, rhs: bool) -> int { lhs - rhs; 0 }\nfunction mul_int_bool(lhs: int, rhs: bool) -> int { lhs * rhs; 0 }\nfunction div_int_bool(lhs: int, rhs: bool) -> int { lhs / rhs; 0 }\nfunction rem_int_bool(lhs: int, rhs: bool) -> int { lhs % rhs; 0 }\nfunction add_int_null(lhs: int, rhs: null) -> int { lhs + rhs; 0 }\nfunction sub_int_null(lhs: int, rhs: null) -> int { lhs - rhs; 0 }\nfunction mul_int_null(lhs: int, rhs: null) -> int { lhs * rhs; 0 }\nfunction div_int_null(lhs: int, rhs: null) -> int { lhs / rhs; 0 }\nfunction rem_int_null(lhs: int, rhs: null) -> int { lhs % rhs; 0 }\nfunction add_float_bigint(lhs: float, rhs: bigint) -> int { lhs + rhs; 0 }\nfunction sub_float_bigint(lhs: float, rhs: bigint) -> int { lhs - rhs; 0 }\nfunction mul_float_bigint(lhs: float, rhs: bigint) -> int { lhs * rhs; 0 }\nfunction div_float_bigint(lhs: float, rhs: bigint) -> int { lhs / rhs; 0 }\nfunction rem_float_bigint(lhs: float, rhs: bigint) -> int { lhs % rhs; 0 }\nfunction add_float_string(lhs: float, rhs: string) -> int { lhs + rhs; 0 }\nfunction sub_float_string(lhs: float, rhs: string) -> int { lhs - rhs; 0 }\nfunction mul_float_string(lhs: float, rhs: string) -> int { lhs * rhs; 0 }\nfunction div_float_string(lhs: float, rhs: string) -> int { lhs / rhs; 0 }\nfunction rem_float_string(lhs: float, rhs: string) -> int { lhs % rhs; 0 }\nfunction add_float_bool(lhs: float, rhs: bool) -> int { lhs + rhs; 0 }\nfunction sub_float_bool(lhs: float, rhs: bool) -> int { lhs - rhs; 0 }\nfunction mul_float_bool(lhs: float, rhs: bool) -> int { lhs * rhs; 0 }\nfunction div_float_bool(lhs: float, rhs: bool) -> int { lhs / rhs; 0 }\nfunction rem_float_bool(lhs: float, rhs: bool) -> int { lhs % rhs; 0 }\nfunction add_float_null(lhs: float, rhs: null) -> int { lhs + rhs; 0 }\nfunction sub_float_null(lhs: float, rhs: null) -> int { lhs - rhs; 0 }\nfunction mul_float_null(lhs: float, rhs: null) -> int { lhs * rhs; 0 }\nfunction div_float_null(lhs: float, rhs: null) -> int { lhs / rhs; 0 }\nfunction rem_float_null(lhs: float, rhs: null) -> int { lhs % rhs; 0 }\nfunction add_bigint_float(lhs: bigint, rhs: float) -> int { lhs + rhs; 0 }\nfunction sub_bigint_float(lhs: bigint, rhs: float) -> int { lhs - rhs; 0 }\nfunction mul_bigint_float(lhs: bigint, rhs: float) -> int { lhs * rhs; 0 }\nfunction div_bigint_float(lhs: bigint, rhs: float) -> int { lhs / rhs; 0 }\nfunction rem_bigint_float(lhs: bigint, rhs: float) -> int { lhs % rhs; 0 }\nfunction add_bigint_string(lhs: bigint, rhs: string) -> int { lhs + rhs; 0 }\nfunction sub_bigint_string(lhs: bigint, rhs: string) -> int { lhs - rhs; 0 }\nfunction mul_bigint_string(lhs: bigint, rhs: string) -> int { lhs * rhs; 0 }\nfunction div_bigint_string(lhs: bigint, rhs: string) -> int { lhs / rhs; 0 }\nfunction rem_bigint_string(lhs: bigint, rhs: string) -> int { lhs % rhs; 0 }\nfunction add_bigint_bool(lhs: bigint, rhs: bool) -> int { lhs + rhs; 0 }\nfunction sub_bigint_bool(lhs: bigint, rhs: bool) -> int { lhs - rhs; 0 }\nfunction mul_bigint_bool(lhs: bigint, rhs: bool) -> int { lhs * rhs; 0 }\nfunction div_bigint_bool(lhs: bigint, rhs: bool) -> int { lhs / rhs; 0 }\nfunction rem_bigint_bool(lhs: bigint, rhs: bool) -> int { lhs % rhs; 0 }\nfunction add_bigint_null(lhs: bigint, rhs: null) -> int { lhs + rhs; 0 }\nfunction sub_bigint_null(lhs: bigint, rhs: null) -> int { lhs - rhs; 0 }\nfunction mul_bigint_null(lhs: bigint, rhs: null) -> int { lhs * rhs; 0 }\nfunction div_bigint_null(lhs: bigint, rhs: null) -> int { lhs / rhs; 0 }\nfunction rem_bigint_null(lhs: bigint, rhs: null) -> int { lhs % rhs; 0 }\nfunction add_string_int(lhs: string, rhs: int) -> int { lhs + rhs; 0 }\nfunction sub_string_int(lhs: string, rhs: int) -> int { lhs - rhs; 0 }\nfunction mul_string_int(lhs: string, rhs: int) -> int { lhs * rhs; 0 }\nfunction div_string_int(lhs: string, rhs: int) -> int { lhs / rhs; 0 }\nfunction rem_string_int(lhs: string, rhs: int) -> int { lhs % rhs; 0 }\nfunction add_string_float(lhs: string, rhs: float) -> int { lhs + rhs; 0 }\nfunction sub_string_float(lhs: string, rhs: float) -> int { lhs - rhs; 0 }\nfunction mul_string_float(lhs: string, rhs: float) -> int { lhs * rhs; 0 }\nfunction div_string_float(lhs: string, rhs: float) -> int { lhs / rhs; 0 }\nfunction rem_string_float(lhs: string, rhs: float) -> int { lhs % rhs; 0 }\nfunction add_string_bigint(lhs: string, rhs: bigint) -> int { lhs + rhs; 0 }\nfunction sub_string_bigint(lhs: string, rhs: bigint) -> int { lhs - rhs; 0 }\nfunction mul_string_bigint(lhs: string, rhs: bigint) -> int { lhs * rhs; 0 }\nfunction div_string_bigint(lhs: string, rhs: bigint) -> int { lhs / rhs; 0 }\nfunction rem_string_bigint(lhs: string, rhs: bigint) -> int { lhs % rhs; 0 }\nfunction sub_string_string(lhs: string, rhs: string) -> int { lhs - rhs; 0 }\nfunction mul_string_string(lhs: string, rhs: string) -> int { lhs * rhs; 0 }\nfunction div_string_string(lhs: string, rhs: string) -> int { lhs / rhs; 0 }\nfunction rem_string_string(lhs: string, rhs: string) -> int { lhs % rhs; 0 }\nfunction add_string_bool(lhs: string, rhs: bool) -> int { lhs + rhs; 0 }\nfunction sub_string_bool(lhs: string, rhs: bool) -> int { lhs - rhs; 0 }\nfunction mul_string_bool(lhs: string, rhs: bool) -> int { lhs * rhs; 0 }\nfunction div_string_bool(lhs: string, rhs: bool) -> int { lhs / rhs; 0 }\nfunction rem_string_bool(lhs: string, rhs: bool) -> int { lhs % rhs; 0 }\nfunction add_string_null(lhs: string, rhs: null) -> int { lhs + rhs; 0 }\nfunction sub_string_null(lhs: string, rhs: null) -> int { lhs - rhs; 0 }\nfunction mul_string_null(lhs: string, rhs: null) -> int { lhs * rhs; 0 }\nfunction div_string_null(lhs: string, rhs: null) -> int { lhs / rhs; 0 }\nfunction rem_string_null(lhs: string, rhs: null) -> int { lhs % rhs; 0 }\nfunction add_bool_int(lhs: bool, rhs: int) -> int { lhs + rhs; 0 }\nfunction sub_bool_int(lhs: bool, rhs: int) -> int { lhs - rhs; 0 }\nfunction mul_bool_int(lhs: bool, rhs: int) -> int { lhs * rhs; 0 }\nfunction div_bool_int(lhs: bool, rhs: int) -> int { lhs / rhs; 0 }\nfunction rem_bool_int(lhs: bool, rhs: int) -> int { lhs % rhs; 0 }\nfunction add_bool_float(lhs: bool, rhs: float) -> int { lhs + rhs; 0 }\nfunction sub_bool_float(lhs: bool, rhs: float) -> int { lhs - rhs; 0 }\nfunction mul_bool_float(lhs: bool, rhs: float) -> int { lhs * rhs; 0 }\nfunction div_bool_float(lhs: bool, rhs: float) -> int { lhs / rhs; 0 }\nfunction rem_bool_float(lhs: bool, rhs: float) -> int { lhs % rhs; 0 }\nfunction add_bool_bigint(lhs: bool, rhs: bigint) -> int { lhs + rhs; 0 }\nfunction sub_bool_bigint(lhs: bool, rhs: bigint) -> int { lhs - rhs; 0 }\nfunction mul_bool_bigint(lhs: bool, rhs: bigint) -> int { lhs * rhs; 0 }\nfunction div_bool_bigint(lhs: bool, rhs: bigint) -> int { lhs / rhs; 0 }\nfunction rem_bool_bigint(lhs: bool, rhs: bigint) -> int { lhs % rhs; 0 }\nfunction add_bool_string(lhs: bool, rhs: string) -> int { lhs + rhs; 0 }\nfunction sub_bool_string(lhs: bool, rhs: string) -> int { lhs - rhs; 0 }\nfunction mul_bool_string(lhs: bool, rhs: string) -> int { lhs * rhs; 0 }\nfunction div_bool_string(lhs: bool, rhs: string) -> int { lhs / rhs; 0 }\nfunction rem_bool_string(lhs: bool, rhs: string) -> int { lhs % rhs; 0 }\nfunction add_bool_bool(lhs: bool, rhs: bool) -> int { lhs + rhs; 0 }\nfunction sub_bool_bool(lhs: bool, rhs: bool) -> int { lhs - rhs; 0 }\nfunction mul_bool_bool(lhs: bool, rhs: bool) -> int { lhs * rhs; 0 }\nfunction div_bool_bool(lhs: bool, rhs: bool) -> int { lhs / rhs; 0 }\nfunction rem_bool_bool(lhs: bool, rhs: bool) -> int { lhs % rhs; 0 }\nfunction add_bool_null(lhs: bool, rhs: null) -> int { lhs + rhs; 0 }\nfunction sub_bool_null(lhs: bool, rhs: null) -> int { lhs - rhs; 0 }\nfunction mul_bool_null(lhs: bool, rhs: null) -> int { lhs * rhs; 0 }\nfunction div_bool_null(lhs: bool, rhs: null) -> int { lhs / rhs; 0 }\nfunction rem_bool_null(lhs: bool, rhs: null) -> int { lhs % rhs; 0 }\nfunction add_null_int(lhs: null, rhs: int) -> int { lhs + rhs; 0 }\nfunction sub_null_int(lhs: null, rhs: int) -> int { lhs - rhs; 0 }\nfunction mul_null_int(lhs: null, rhs: int) -> int { lhs * rhs; 0 }\nfunction div_null_int(lhs: null, rhs: int) -> int { lhs / rhs; 0 }\nfunction rem_null_int(lhs: null, rhs: int) -> int { lhs % rhs; 0 }\nfunction add_null_float(lhs: null, rhs: float) -> int { lhs + rhs; 0 }\nfunction sub_null_float(lhs: null, rhs: float) -> int { lhs - rhs; 0 }\nfunction mul_null_float(lhs: null, rhs: float) -> int { lhs * rhs; 0 }\nfunction div_null_float(lhs: null, rhs: float) -> int { lhs / rhs; 0 }\nfunction rem_null_float(lhs: null, rhs: float) -> int { lhs % rhs; 0 }\nfunction add_null_bigint(lhs: null, rhs: bigint) -> int { lhs + rhs; 0 }\nfunction sub_null_bigint(lhs: null, rhs: bigint) -> int { lhs - rhs; 0 }\nfunction mul_null_bigint(lhs: null, rhs: bigint) -> int { lhs * rhs; 0 }\nfunction div_null_bigint(lhs: null, rhs: bigint) -> int { lhs / rhs; 0 }\nfunction rem_null_bigint(lhs: null, rhs: bigint) -> int { lhs % rhs; 0 }\nfunction add_null_string(lhs: null, rhs: string) -> int { lhs + rhs; 0 }\nfunction sub_null_string(lhs: null, rhs: string) -> int { lhs - rhs; 0 }\nfunction mul_null_string(lhs: null, rhs: string) -> int { lhs * rhs; 0 }\nfunction div_null_string(lhs: null, rhs: string) -> int { lhs / rhs; 0 }\nfunction rem_null_string(lhs: null, rhs: string) -> int { lhs % rhs; 0 }\nfunction add_null_bool(lhs: null, rhs: bool) -> int { lhs + rhs; 0 }\nfunction sub_null_bool(lhs: null, rhs: bool) -> int { lhs - rhs; 0 }\nfunction mul_null_bool(lhs: null, rhs: bool) -> int { lhs * rhs; 0 }\nfunction div_null_bool(lhs: null, rhs: bool) -> int { lhs / rhs; 0 }\nfunction rem_null_bool(lhs: null, rhs: bool) -> int { lhs % rhs; 0 }\nfunction add_null_null(lhs: null, rhs: null) -> int { lhs + rhs; 0 }\nfunction sub_null_null(lhs: null, rhs: null) -> int { lhs - rhs; 0 }\nfunction mul_null_null(lhs: null, rhs: null) -> int { lhs * rhs; 0 }\nfunction div_null_null(lhs: null, rhs: null) -> int { lhs / rhs; 0 }\nfunction rem_null_null(lhs: null, rhs: null) -> int { lhs % rhs; 0 }\n",
        },
        "cannot be applied",
    )
}

test "structural_arithmetic_without_impls_is_rejected" {
    AssertErrorCount(
        {
            "structural_arithmetic_without_impls_is_rejected.baml": "\n        function f(xs: int[], values: map<string, int>, n: float) -> int {\n            xs + n;\n            n + xs;\n            values + n;\n            n + values;\n            0\n        }\n        ",
        },
        4,
    );
    AssertAllErrorsContain(
        {
            "structural_arithmetic_without_impls_is_rejected.baml": "\n        function f(xs: int[], values: map<string, int>, n: float) -> int {\n            xs + n;\n            n + xs;\n            values + n;\n            n + values;\n            0\n        }\n        ",
        },
        "cannot be applied",
    )
}

test "conjunction_ambiguous_source_fields_diagnostic_names_ambiguity" {
    AssertHasDiagnostic(
        {
            "conjunction_ambiguous_source_fields_diagnostic_names_ambiguity.baml": "\n        interface Left { value: string }\n        interface Right { value: int }\n        function read<T extends Left & Right>(value: T) -> string {\n            return value.value\n        }\n        ",
        },
        "",
        ["field `value` on class", "is ambiguous because it is declared by multiple interfaces"],
    )
}

test "ordering_on_different_concrete_types_is_rejected" {
    AssertHasDiagnostic(
        {
            "ordering_on_different_concrete_types_is_rejected.baml": "\n        function f(a: int, b: string) -> bool {\n            a < b\n        }\n        ",
        },
        "",
        ["ordering requires both operands", "with `<`"],
    )
}

test "wf3_bare_generic_interface_reflection_is_arity_error" {
    root.compiler.AssertHasMessage(
        {
            "wf3_bare_generic_interface_reflection_is_arity_error.baml": "\n        interface Box<T> {\n            function get(self) -> T throws never\n        }\n        class IntBox {\n            implements Box<int> {\n                function get(self) -> int { return 1 }\n            }\n        }\n        function main() -> bool {\n            let bare = reflect.Type.of<Box>()\n            return bare.implemented_by(reflect.Type.of<IntBox>())\n        }\n        ",
        },
        "type `Box` expects 1 type argument(s), got 0",
    )
}
