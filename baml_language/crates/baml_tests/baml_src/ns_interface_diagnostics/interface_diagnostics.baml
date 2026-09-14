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
            "missing_required_method_is_compile_error.baml": `
                interface Animal {
                    function speak(self) -> string throws never
                }

                class Incomplete {
                    implements Animal {}
                }
            `,
        },
        "E0113",
    )
}

test "missing_required_method_message_names_method_and_interface" {
    root.compiler.AssertHasMessage(
        {
            "missing_required_method_message_names_method_and_interface.baml": `
                interface Animal {
                    function speak(self) -> string throws never
                }
                class Mute {
                    implements Animal {}
                }
            `,
        },
        "method `speak` required by interface `Animal`",
    )
}

test "implements_unknown_interface_is_compile_error" {
    root.compiler.AssertHasCode(
        {
            "implements_unknown_interface_is_compile_error.baml": `
                class Ghost {
                    implements DoesNotExist {}
                }
            `,
        },
        "E0002",
    )
}

test "implementing_a_class_is_not_allowed" {
    root.compiler.AssertHasCode(
        {
            "implementing_a_class_is_not_allowed.baml": `
                class Foo { x: int }
                class Bar {
                    implements Foo {}
                }
            `,
        },
        "E0119",
    )
}

test "implementing_an_enum_is_not_allowed" {
    root.compiler.AssertHasCode(
        {
            "implementing_an_enum_is_not_allowed.baml": `
                enum Color { Red, Green, Blue }
                class Bar {
                    implements Color {}
                }
            `,
        },
        "E0119",
    )
}

test "orphan_rule_rejects_blanket_impl_of_foreign_interface" {
    root.compiler.AssertHasCode(
        {
            "orphan_rule_rejects_blanket_impl_of_foreign_interface.baml": `
                implement<T> baml.ops.Equals for T {}
            `,
        },
        "E0139",
    )
}

test "orphan_rule_rejects_foreign_interface_for_foreign_type" {
    root.compiler.AssertHasCode(
        {
            "orphan_rule_rejects_foreign_interface_for_foreign_type.baml": `
                implement baml.ops.Equals for int {}
            `,
        },
        "E0139",
    )
}

test "unknown_method_in_implements_block_is_compile_error" {
    root.compiler.AssertHasCode(
        {
            "unknown_method_in_implements_block_is_compile_error.baml": `
                interface Animal {
                    function speak(self) -> string throws never
                }
                class Dog {
                    implements Animal {
                        function speak(self) -> string { return "woof" }
                        function bark(self) -> string { return "bark" }
                    }
                }
            `,
        },
        "E0115",
    )
}

test "duplicate_implements_block_is_compile_error" {
    root.compiler.AssertHasCode(
        {
            "duplicate_implements_block_is_compile_error.baml": `
                interface Animal {
                    function speak(self) -> string throws never
                }

                class Dog {
                    implements Animal {
                        function speak(self) -> string { return "a" }
                    }
                    implements Animal {
                        function speak(self) -> string { return "b" }
                    }
                }
            `,
        },
        "E0132",
    )
}

test "duplicate_same_generic_interface_instantiation_is_compile_error" {
    root.compiler.AssertHasCode(
        {
            "duplicate_same_generic_interface_instantiation_is_compile_error.baml": `
                interface Converter<T> {
                    function convert(self) -> T throws never
                }

                class MultiFormat {
                    implements Converter<int> {
                        function convert(self) -> int { return 1 }
                    }
                    implements Converter<int> {
                        function convert(self) -> int { return 2 }
                    }
                }
            `,
        },
        "E0132",
    )
}

test "duplicate_generic_interface_instantiation_via_alias_is_compile_error" {
    root.compiler.AssertHasCode(
        {
            "duplicate_generic_interface_instantiation_via_alias_is_compile_error.baml": `
                interface Converter<T> {
                    function convert(self) -> T throws never
                }

                type IntAlias = int

                class MultiFormat {
                    implements Converter<IntAlias> {
                        function convert(self) -> int { return 1 }
                    }
                    implements Converter<int> {
                        function convert(self) -> int { return 2 }
                    }
                }
            `,
        },
        "E0132",
    )
}

test "nested_alias_duplicate_implements_is_detected" {
    root.compiler.AssertHasCode(
        {
            "nested_alias_duplicate_implements_is_detected.baml": `
                interface Converter<T> {
                    function convert(self) -> T throws never
                }

                type IntList = int[]

                class MultiFormat {
                    implements Converter<IntList[]> {
                        function convert(self) -> int[][] { return [[1]] }
                    }
                    implements Converter<int[][]> {
                        function convert(self) -> int[][] { return [[2]] }
                    }
                }
            `,
        },
        "E0132",
    )
}

test "requires_cycle_is_compile_error" {
    root.compiler.AssertHasCode(
        {
            "requires_cycle_is_compile_error.baml": `
                interface A requires B {}
                interface B requires A {}
            `,
        },
        "E0118",
    )
}

test "three_way_requires_cycle_is_compile_error" {
    root.compiler.AssertHasCode(
        {
            "three_way_requires_cycle_is_compile_error.baml": `
                interface A requires B {}
                interface B requires C {}
                interface C requires A {}
            `,
        },
        "E0118",
    )
}

test "missing_class_field_for_interface_field_is_error" {
    root.compiler.AssertHasCode(
        {
            "missing_class_field_for_interface_field_is_error.baml": `
                interface Config {
                    host: string
                    port: int
                }

                class Server {
                    host: string
                    implements Config {}
                }
            `,
        },
        "E0124",
    )
}

test "interface_extends_clause_is_rejected" {
    root.compiler.AssertHasMessage(
        {
            "interface_extends_clause_is_rejected.baml": `
                interface Named {
                    name: string
                }
                interface Person extends Named {
                    age: int
                }
            `,
        },
        "extends",
    )
}

test "implementing_requires_chain_satisfies_parent_required_methods" {
    root.compiler.AssertHasMessage(
        {
            "implementing_requires_chain_satisfies_parent_required_methods.baml": `
                interface Named { name: string }
                interface Aged { age: int }
                interface Person requires Named, Aged {
                    occupation: string
                    function introduce(self) -> string throws never
                }

                class Employee {
                    salary: float
                    implements Person {}
                }
            `,
        },
        "method `introduce` required by interface `Person`",
    )
}

test "requires_chain_required_method_must_be_provided" {
    root.compiler.AssertHasMessage(
        {
            "requires_chain_required_method_must_be_provided.baml": `
                interface Greeter {
                    function greet(self) -> string throws never
                }
                interface Person requires Greeter {
                    name: string
                }

                class Bob {
                    implements Person {}
                }
            `,
        },
        "also requires implementing `Greeter`",
    )
}

test "class_without_implements_cannot_satisfy_interface_param" {
    root.compiler.AssertHasMessage(
        {
            "class_without_implements_cannot_satisfy_interface_param.baml": `
                interface Animal {
                    function speak(self) -> string throws never
                }
                // Note: no \`implements Animal\`.
                class Robot {
                    function speak(self) -> string { return "beep" }
                }

                function describe(a: Animal) -> string {
                    return "an animal"
                }

                function main() -> string {
                    let r = Robot {}
                    return describe(r)
                }
            `,
        },
        "Robot",
    )
}

test "method_signature_mismatch_return_type" {
    root.compiler.AssertHasCode(
        {
            "method_signature_mismatch_return_type.baml": `
                interface Animal {
                    function speak(self) -> string throws never
                }
                class Robot {
                    implements Animal {
                        function speak(self) -> int { return 42 }
                    }
                }
            `,
        },
        "E0120",
    )
}

test "method_signature_mismatch_param_type" {
    root.compiler.AssertHasCode(
        {
            "method_signature_mismatch_param_type.baml": `
                interface Adder {
                    function add(self, a: int, b: int) -> int throws never
                }
                class BadAdder {
                    implements Adder {
                        function add(self, a: string, b: string) -> int { return 0 }
                    }
                }
            `,
        },
        "E0120",
    )
}

test "class_own_method_does_not_resolve_ambiguous_interface_method_call" {
    root.compiler.AssertHasCode(
        {
            "class_own_method_does_not_resolve_ambiguous_interface_method_call.baml": `
                interface Serializer {
                    function encode(self) -> string throws never
                }
                interface BinarySerializer {
                    function encode(self) -> string throws never
                }
                class Hybrid {
                    function encode(self) -> string { return "class" }
                    implements Serializer {
                        function encode(self) -> string { return "json" }
                    }
                    implements BinarySerializer {
                        function encode(self) -> string { return "bin" }
                    }
                }
                function main() -> string {
                    let h = Hybrid {}
                    return h.encode()
                }
            `,
        },
        "E0121",
    )
}

test "qualified_interface_field_construction_is_compile_error" {
    root.compiler.AssertHasMessage(
        {
            "qualified_interface_field_construction_is_compile_error.baml": `
                interface Config {
                    host: string
                }
                class Server {
                    host: string
                    implements Config {}
                }
                function main() -> string {
                    let s = Server { Config.host: "localhost" }
                    return s.host
                }
            `,
        },
        "Config.host",
    )
}

test "simple_interface_field_view_construction_is_compile_error" {
    root.compiler.AssertHasMessage(
        {
            "simple_interface_field_view_construction_is_compile_error.baml": `
                interface Named {
                    name: string
                }
                class Person {
                    title: string
                    implements Named {
                        name as title
                    }
                }
                function main() -> string {
                    let p = Person { name: "Ada" }
                    return p.title
                }
            `,
        },
        "use class field `title`",
    )
}

test "interface_field_type_must_match_invariantly" {
    root.compiler.AssertHasCode(
        {
            "interface_field_type_must_match_invariantly.baml": `
                interface Measured {
                    value: int | string
                }
                class Count {
                    value: int
                    implements Measured {}
                }
            `,
        },
        "E0116",
    )
}

test "unqualified_same_name_interface_field_access_is_ambiguous" {
    root.compiler.AssertHasMessage(
        {
            "unqualified_same_name_interface_field_access_is_ambiguous.baml": `
                interface Named { name: string }
                interface Labeled { name: string }
                class Item {
                    named_name: string
                    labeled_name: string
                    implements Named { name as named_name }
                    implements Labeled { name as labeled_name }
                }
                function main() -> string {
                    let i = Item { named_name: "widget", labeled_name: "WIDGET-001" }
                    return i.name
                }
            `,
        },
        "as<Named>",
    )
}

test "generic_interface_field_links_swapped_type_var_impls_overlap" {
    root.compiler.AssertHasCode(
        {
            "generic_interface_field_links_swapped_type_var_impls_overlap.baml": `
                interface Slot<T, E> {
                    value: T
                }
                class Pair<L, R> {
                    left: L
                    right: R
                    implements Slot<L, R> {
                        value as left
                    }
                    implements Slot<R, L> {
                        value as right
                    }
                }
                function main() -> bool {
                    let p: Pair<int, string> = Pair { left: 7, right: "seven" }
                    let lr: Slot<int, string> = p
                    let rl: Slot<string, int> = p
                    return lr.value == 7 && rl.value == "seven"
                }
            `,
        },
        "E0132",
    )
}

test "generic_interface_method_swapped_type_var_impls_overlap" {
    root.compiler.AssertHasCode(
        {
            "generic_interface_method_swapped_type_var_impls_overlap.baml": `
                interface Reporter<T, E> {
                    function show(self) -> T throws never
                }
                class Pair<L, R> {
                    left: L
                    right: R
                    implements Reporter<L, R> {
                        function show(self) -> L {
                            return self.left
                        }
                    }
                    implements Reporter<R, L> {
                        function show(self) -> R {
                            return self.right
                        }
                    }
                }
                function main() -> bool {
                    let p: Pair<int, string> = Pair { left: 7, right: "seven" }
                    let lr: Reporter<int, string> = p
                    let rl: Reporter<string, int> = p
                    return lr.show() == 7 && rl.show() == "seven"
                }
            `,
        },
        "E0132",
    )
}

test "old_interface_qualified_projection_is_compile_error" {
    root.compiler.AssertHasMessage(
        {
            "old_interface_qualified_projection_is_compile_error.baml": `
                interface Animal {
                    function speak(self) -> string throws never
                }
                class Dog {
                    implements Animal {
                        function speak(self) -> string { return "Woof!" }
                    }
                }
                function bad() -> string {
                    let d = Dog {}
                    return d.Animal.speak()
                }
            `,
        },
        "has no member `Animal`",
    )
}

test "self_param_method_rejects_heterogeneous_generic_args" {
    root.compiler.AssertHasMessage(
        {
            "self_param_method_rejects_heterogeneous_generic_args.baml": `
                interface Equatable {
                    function eq(self, other: Self) -> bool throws never
                }
                function cmp<S extends Equatable, U extends Equatable>(x: S, y: U) -> bool {
                    return x.eq(y)
                }
            `,
        },
        "expected `S`, found `U`",
    )
}

test "self_param_method_rejects_mismatched_literal_arg" {
    root.compiler.AssertHasMessage(
        {
            "self_param_method_rejects_mismatched_literal_arg.baml": `
                interface Equatable {
                    function eq(self, other: Self) -> bool throws never
                }
                function bad<T extends Equatable>(x: T) -> bool {
                    return x.eq(5)
                }
            `,
        },
        "expected `T`, found `5`",
    )
}

test "self_param_method_rejects_nested_self_mismatch" {
    root.compiler.AssertHasMessage(
        {
            "self_param_method_rejects_nested_self_mismatch.baml": `
                interface Adder {
                    function addAll(self, others: Self[]) -> int throws never
                }
                function cross<T extends Adder, U extends Adder>(x: T, ys: U[]) -> int {
                    return x.addAll(ys)
                }
            `,
        },
        "expected `T[]`, found `U[]`",
    )
}

test "bound_self_method_value_rejects_heterogeneous_arg" {
    root.compiler.AssertHasMessage(
        {
            "bound_self_method_value_rejects_heterogeneous_arg.baml": `
                interface Adder {
                    function addOne(self, other: Self) -> int throws never
                }
                function cross<T extends Adder, U extends Adder>(x: T, y: U) -> int {
                    let f = x.addOne
                    return f(y)
                }
            `,
        },
        "expected `T`, found `U`",
    )
}

test "mixed_interface_generic_overlapping_type_arg_impls_overlap" {
    root.compiler.AssertHasCode(
        {
            "mixed_interface_generic_overlapping_type_arg_impls_overlap.baml": `
                interface Slot<T> {
                    value: T
                }
                class Pair<L, R> {
                    left: L
                    right: R
                    implements Slot<L> {
                        value as left
                    }
                    implements Slot<R> {
                        value as right
                    }
                }

                function describe(s: Slot<string>) -> string {
                    return match (s) {
                        Pair<int, string> { left: 7 } => "pair"
                        Slot<string> { value: string } => "slot"
                    }
                }
            `,
        },
        "E0132",
    )
}

test "generic_interface_and_concrete_destructures_still_report_partial_coverage" {
    root.compiler.AssertHasCode(
        {
            "generic_interface_and_concrete_destructures_still_report_partial_coverage.baml": `
                interface Slot<T> {
                    value: T
                }
                class Pair<L, R> {
                    left: L
                    right: R
                    implements Slot<L> {
                        value as left
                    }
                }

                function describe(s: Slot<int>) -> string {
                    return match (s) {
                        Pair<int, string> { right: "seven" } => "pair"
                        Slot<int> { value: 1 } => "one"
                    }
                }
            `,
        },
        "E0062",
    )
}

test "union_of_generic_interfaces_reports_missing_interface_branch" {
    root.compiler.AssertHasCode(
        {
            "union_of_generic_interfaces_reports_missing_interface_branch.baml": `
                interface Slot<T> {
                    value: T
                }
                class Pair<L, R> {
                    left: L
                    right: R
                    implements Slot<L> {
                        value as left
                    }
                }

                interface Cargo<T> {
                    payload: T
                }
                class Box<T> {
                    inner: T
                    label: string
                    implements Cargo<T> {
                        payload as inner
                    }
                }

                function describe(x: Slot<int> | Cargo<string>) -> string {
                    return match (x) {
                        Pair<int, string> { right: "tag" } => "pair"
                        Slot<int> { value: int } => "slot"
                    }
                }
            `,
        },
        "E0062",
    )
}

test "mixed_interface_and_concrete_destructures_still_report_partial_coverage" {
    root.compiler.AssertHasCode(
        {
            "mixed_interface_and_concrete_destructures_still_report_partial_coverage.baml": `
                interface Animal {
                    name: string
                    friendly: bool
                }
                class Dog {
                    name: string
                    friendly: bool
                    implements Animal {}
                }

                function describe(a: Animal) -> string {
                    return match (a) {
                        Dog { friendly: true } => "friendly"
                        Animal { name: "Rex" } => "rex"
                    }
                }
            `,
        },
        "E0062",
    )
}

test "class_inherent_method_does_not_satisfy_interface_method" {
    root.compiler.AssertHasCode(
        {
            "class_inherent_method_does_not_satisfy_interface_method.baml": `
                interface Greeter {
                    function greet(self) -> string throws never
                }

                class Person {
                    implements Greeter {}

                    function greet(self) -> int {
                        return 1
                    }
                }
            `,
        },
        "E0113",
    )
}

test "generic_bound_violation_is_compile_error" {
    root.compiler.AssertHasMessage(
        {
            "generic_bound_violation_is_compile_error.baml": `
                interface Named { name: string }
                function first_name<T extends Named>(items: T[]) -> string {
                    return items[0].name
                }
                function main() -> string {
                    let xs: int[] = [1, 2, 3]
                    return first_name<int>(xs)
                }
            `,
        },
        "Named",
    )
}

test "generic_bound_violation_in_instantiation_expr_is_compile_error" {
    root.compiler.AssertHasMessage(
        {
            "generic_bound_violation_in_instantiation_expr_is_compile_error.baml": `
                interface Named { name: string }
                function first_name<T extends Named>(items: T[]) -> string {
                    return items[0].name
                }
                function main() -> string {
                    let f = first_name<int>;
                    return "ok"
                }
            `,
        },
        "Named",
    )
}

test "generic_bound_alias_syntax_is_compile_error" {
    root.compiler.AssertHasMessage(
        {
            "generic_bound_alias_syntax_is_compile_error.baml": `
                interface Converter<T> {
                    function convert(self) -> T throws never
                }
                function read_int<T extends Converter<int> as Ints>(m: T) -> int {
                    return m.as<Converter<int>>.convert()
                }
            `,
        },
        "generic bound aliases are not supported",
    )
}

test "same_interface_different_type_args_is_not_assignable" {
    root.compiler.AssertHasMessage(
        {
            "same_interface_different_type_args_is_not_assignable.baml": `
                interface Box<T> {
                    function get(self) -> T throws never
                }
                function bad(x: Box<int>) -> Box<string> {
                    return x
                }
            `,
        },
        "mismatched types",
    )
}

test "generic_interface_method_explicit_type_args_are_checked" {
    root.compiler.AssertHasMessage(
        {
            "generic_interface_method_explicit_type_args_are_checked.baml": `
                interface Echo<T> {
                    function echo<U>(self, value: U) -> U throws never
                }
                class Echoer {
                    implements Echo<int> {
                        function echo<U>(self, value: U) -> U {
                            return value
                        }
                    }
                }
                function bad() -> string {
                    let e: Echo<int> = Echoer {}
                    return e.echo<int>("nope")
                }
            `,
        },
        "mismatched types",
    )
}

test "class_type_reference_rejects_wrong_generic_arg_count" {
    root.compiler.AssertHasMessage(
        {
            "class_type_reference_rejects_wrong_generic_arg_count.baml": `
                class Box<T> {
                    value: T
                }
                function bad(value: Box<int, string>) -> int {
                    return 1
                }
            `,
        },
        "expects 1 type argument(s), got 2",
    )
}

test "class_type_reference_rejects_too_few_explicit_generic_arg_count" {
    root.compiler.AssertHasMessage(
        {
            "class_type_reference_rejects_too_few_explicit_generic_arg_count.baml": `
                class Pair<L, R> {
                    left: L
                    right: R
                }
                function bad(value: Pair<int>) -> int {
                    return 1
                }
            `,
        },
        "expects 2 type argument(s), got 1",
    )
}

test "function_call_rejects_wrong_explicit_generic_arg_count" {
    root.compiler.AssertHasMessage(
        {
            "function_call_rejects_wrong_explicit_generic_arg_count.baml": `
                function id<T>(value: T) -> T {
                    return value
                }
                function bad() -> int {
                    return id<int, string>(1)
                }
            `,
        },
        "function `id` expects 1 type argument(s), got 2",
    )
}

test "function_call_rejects_too_few_explicit_generic_arg_count" {
    root.compiler.AssertHasMessage(
        {
            "function_call_rejects_too_few_explicit_generic_arg_count.baml": `
                function pair<L, R>(left: L, right: R) -> L {
                    return left
                }
                function bad() -> int {
                    return pair<int>(1, "nope")
                }
            `,
        },
        "function `pair` expects 2 type argument(s), got 1",
    )
}

test "in_body_implements_rejects_wrong_interface_generic_arg_count" {
    root.compiler.AssertHasMessage(
        {
            "in_body_implements_rejects_wrong_interface_generic_arg_count.baml": `
                interface Label<T> {
                    function label(self) -> T throws never
                }
                class Thing {
                    implements Label<int, string> {
                        function label(self) -> int {
                            return 1
                        }
                    }
                }
            `,
        },
        "expects 1 type argument(s), got 2",
    )
}

test "in_body_implements_rejects_too_few_explicit_interface_generic_arg_count" {
    root.compiler.AssertHasMessage(
        {
            "in_body_implements_rejects_too_few_explicit_interface_generic_arg_count.baml": `
                interface PairLabel<L, R> {
                    function label(self) -> L throws never
                }
                class Thing {
                    implements PairLabel<int> {
                        function label(self) -> int {
                            return 1
                        }
                    }
                }
            `,
        },
        "expects 2 type argument(s), got 1",
    )
}

test "out_of_body_implements_rejects_wrong_interface_generic_arg_count" {
    root.compiler.AssertHasMessage(
        {
            "out_of_body_implements_rejects_wrong_interface_generic_arg_count.baml": `
                interface Label<T> {
                    function label(self) -> T throws never
                }
                class Thing {}
                implements Label<int, string> for Thing {
                    function label(self) -> int {
                        return 1
                    }
                }
            `,
        },
        "expects 1 type argument(s), got 2",
    )
}

test "out_of_body_implements_rejects_too_few_explicit_interface_generic_arg_count" {
    root.compiler.AssertHasMessage(
        {
            "out_of_body_implements_rejects_too_few_explicit_interface_generic_arg_count.baml": `
                interface PairLabel<L, R> {
                    function label(self) -> L throws never
                }
                class Thing {}
                implements PairLabel<int> for Thing {
                    function label(self) -> int {
                        return 1
                    }
                }
            `,
        },
        "expects 2 type argument(s), got 1",
    )
}

test "out_of_body_implements_rejects_wrong_target_class_generic_arg_count" {
    root.compiler.AssertHasMessage(
        {
            "out_of_body_implements_rejects_wrong_target_class_generic_arg_count.baml": `
                interface Marker {}
                class Box<T> {
                    value: T
                }
                implements Marker for Box<int, string> {}
            `,
        },
        "expects 1 type argument(s), got 2",
    )
}

test "out_of_body_implements_rejects_too_few_explicit_target_class_generic_arg_count" {
    root.compiler.AssertHasMessage(
        {
            "out_of_body_implements_rejects_too_few_explicit_target_class_generic_arg_count.baml": `
                interface Marker {}
                class Pair<L, R> {
                    left: L
                    right: R
                }
                implements Marker for Pair<int> {}
            `,
        },
        "expects 2 type argument(s), got 1",
    )
}

test "required_interface_method_generic_bound_mismatch_is_error" {
    root.compiler.AssertHasCode(
        {
            "required_interface_method_generic_bound_mismatch_is_error.baml": `
                interface Named { name: string }
                interface Reader {
                    function read<T>(self, value: T) -> string throws never
                }
                class ReaderImpl {
                    implements Reader {
                        function read<T extends Named>(self, value: T) -> string {
                            return value.name
                        }
                    }
                }
            `,
        },
        "E0120",
    )
}

test "generic_bounds_reject_compound_type_expressions" {
    root.compiler.AssertHasMessage(
        {
            "generic_bounds_reject_compound_type_expressions.baml": `
                function keep_union<T extends int | string>(x: T) -> int {
                    return 1
                }
                function main() -> int {
                    return 1
                }
            `,
        },
        "is not an interface",
    )
}

test "multi_self_method_rejected_on_interface_typed_receiver" {
    root.compiler.AssertHasMessage(
        {
            "multi_self_method_rejected_on_interface_typed_receiver.baml": `
                interface Equatable {
                    function same(self, other: Self) -> bool throws never
                }
                class Box {
                    value: int
                    implements Equatable {
                        function same(self, other: Self) -> bool {
                            return self.value == other.value
                        }
                    }
                }
                function bad(a: Equatable, b: Equatable) -> bool {
                    return a.same(b)
                }
            `,
        },
        "concrete receiver",
    )
}

test "concrete_receiver_self_param_method_rejects_wrong_arg" {
    root.compiler.AssertHasMessage(
        {
            "concrete_receiver_self_param_method_rejects_wrong_arg.baml": `
                interface Equals {
                    function eq(self, other: Self) -> bool throws never
                    function neq(self, other: Self) -> bool throws never {
                        return !self.eq(other)
                    }
                }
                class Num {
                    v: int
                    implements Equals {
                        function eq(self, other: Self) -> bool { return self.v == other.v }
                    }
                }
                class Other { w: int }
                function main() -> bool {
                    return Num { v: 1 }.neq(Other { w: 2 })
                }
            `,
        },
        "expected `Num`, found `Other`",
    )
}

test "unbounded_generic_forwarded_to_bounded_call_is_rejected" {
    root.compiler.AssertHasMessage(
        {
            "unbounded_generic_forwarded_to_bounded_call_is_rejected.baml": `
                interface Equatable {
                    function eq(self, other: Self) -> bool throws never
                }
                function same<T extends Equatable>(x: T) -> bool {
                    return x.eq(x)
                }
                function forward<U>(x: U) -> bool {
                    return same(x)
                }
            `,
        },
        "expected `Equatable`, found `U`",
    )
}

test "unbounded_generic_forwarded_through_container_is_rejected" {
    root.compiler.AssertHasMessage(
        {
            "unbounded_generic_forwarded_through_container_is_rejected.baml": `
                interface Equatable {
                    function eq(self, other: Self) -> bool throws never
                }
                function firstEq<T extends Equatable>(xs: T[]) -> bool {
                    return xs[0].eq(xs[0])
                }
                function forward<U>(xs: U[]) -> bool {
                    return firstEq(xs)
                }
            `,
        },
        "expected `Equatable`, found `U`",
    )
}

test "self_type_outside_interface_or_class_is_compile_error" {
    root.compiler.AssertHasMessage(
        {
            "self_type_outside_interface_or_class_is_compile_error.baml": `
                function bad(x: Self) -> Self {
                    return x
                }
            `,
        },
        "Self",
    )
}

test "cast_from_one_interface_to_another_when_class_implements_both_is_error" {
    root.compiler.AssertHasMessage(
        {
            "cast_from_one_interface_to_another_when_class_implements_both_is_error.baml": `
                interface Animal {
                    function speak(self) -> string throws never
                }
                interface Swimmer {
                    function swim(self) -> string throws never
                }
                class Dog {
                    implements Animal {
                        function speak(self) -> string { return "Woof!" }
                    }
                    implements Swimmer {
                        function swim(self) -> string { return "splash" }
                    }
                }
                function main() -> string {
                    let a: Animal = Dog {}
                    let s: Swimmer = a
                    return s.swim()
                }
            `,
        },
        "Animal",
    )
}

test "as_rejects_interface_downcast" {
    root.compiler.AssertHasMessage(
        {
            "as_rejects_interface_downcast.baml": `
                interface Animal {
                    function speak(self) -> string throws never
                }
                interface Swimmer {
                    function swim(self) -> string throws never
                }
                class Dog {
                    implements Animal {
                        function speak(self) -> string { return "Woof!" }
                    }
                    implements Swimmer {
                        function swim(self) -> string { return "splash" }
                    }
                }
                function bad(a: Animal) -> string {
                    let s = a.as<Swimmer>
                    return s.swim()
                }
            `,
        },
        "Animal",
    )
}

test "as_requires_interface_target" {
    root.compiler.AssertHasMessage(
        {
            "as_requires_interface_target.baml": `
                interface Animal {
                    function speak(self) -> string throws never
                }
                class Dog {
                    implements Animal {
                        function speak(self) -> string { return "Woof!" }
                    }
                }
                function bad(d: Dog) -> Dog {
                    return d.as<Dog>
                }
            `,
        },
        "expected an interface qualifier",
    )
}

test "default_keyword_outside_implements_block_is_compile_error" {
    root.compiler.AssertHasMessage(
        {
            "default_keyword_outside_implements_block_is_compile_error.baml": `
                interface Logger {
                    function log(self, msg: string) -> string throws never { return msg }
                }
                function rogue(msg: string) -> string {
                    return default.log(msg)
                }
            `,
        },
        "default",
    )
}

test "default_keyword_inside_class_method_outside_implements_is_compile_error" {
    root.compiler.AssertHasMessage(
        {
            "default_keyword_inside_class_method_outside_implements_is_compile_error.baml": `
                interface Logger {
                    function log(self, msg: string) -> string throws never { return msg }
                }
                class Helper {
                    function rogue(self, msg: string) -> string {
                        return default.log(msg)
                    }
                }
            `,
        },
        "default",
    )
}

test "default_keyword_inside_lambda_inside_implements_block" {
    root.compiler.AssertHasMessage(
        {
            "default_keyword_inside_lambda_inside_implements_block.baml": `
                interface Logger {
                    function log(self, msg: string) -> string throws never { return msg }
                }
                class L {
                    implements Logger {
                        function log(self, msg: string) -> string {
                            let f = (m: string) => default.log(m)
                            return f(msg)
                        }
                    }
                }
            `,
        },
        "default",
    )
}

test "missing_class_field_for_interface_with_partial_links_is_error" {
    root.compiler.AssertHasCode(
        {
            "missing_class_field_for_interface_with_partial_links_is_error.baml": `
                interface Config {
                    host: string
                    port: int
                }
                class Server {
                    host: string
                    implements Config {
                        host as host
                    }
                }
            `,
        },
        "E0124",
    )
}

test "declaring_field_in_implements_block_is_error" {
    root.compiler.AssertHasCode(
        {
            "declaring_field_in_implements_block_is_error.baml": `
                interface Config {
                    port: int
                }
                class Server {
                    implements Config {
                        port: string
                    }
                }
            `,
        },
        "E0127",
    )
}

test "unknown_interface_field_in_link_is_error" {
    root.compiler.AssertHasCode(
        {
            "unknown_interface_field_in_link_is_error.baml": `
                interface Named {
                    name: string
                }
                class Person {
                    name: string
                    implements Named {
                        title as name
                    }
                }
            `,
        },
        "E0128",
    )
}

test "unknown_class_field_in_link_is_error" {
    root.compiler.AssertHasCode(
        {
            "unknown_class_field_in_link_is_error.baml": `
                interface Named {
                    name: string
                }
                class Person {
                    name: string
                    implements Named {
                        name as display_name
                    }
                }
            `,
        },
        "E0129",
    )
}

test "duplicate_interface_field_link_is_error" {
    root.compiler.AssertHasCode(
        {
            "duplicate_interface_field_link_is_error.baml": `
                interface Named {
                    name: string
                }
                class Person {
                    primary_name: string
                    secondary_name: string
                    implements Named {
                        name as primary_name
                        name as secondary_name
                    }
                }
            `,
        },
        "E0130",
    )
}

test "missing_required_parent_interface_is_error" {
    root.compiler.AssertHasCode(
        {
            "missing_required_parent_interface_is_error.baml": `
                interface Named { name: string }
                interface Aged { age: int }
                interface Person requires Named, Aged {
                    occupation: string
                }
                class Bad {
                    name: string
                    age: int
                    occupation: string
                    implements Person {}
                }
            `,
        },
        "E0125",
    )
}

test "missing_one_of_two_required_parents_is_error" {
    root.compiler.AssertHasCode(
        {
            "missing_one_of_two_required_parents_is_error.baml": `
                interface Named { name: string }
                interface Aged { age: int }
                interface Person requires Named, Aged {
                    occupation: string
                }
                class Partial {
                    name: string
                    age: int
                    occupation: string
                    implements Named {}
                    implements Person {}
                }
            `,
        },
        "E0125",
    )
}

test "in_body_inherent_method_does_not_implicitly_satisfy_interface" {
    root.compiler.AssertHasCode(
        {
            "in_body_inherent_method_does_not_implicitly_satisfy_interface.baml": `
                interface Label {
                    function label(self, name: string) -> string throws never
                }

                class Thing {
                    function label(self, value: string) -> string {
                        return value
                    }

                    implements Label {}
                }
            `,
        },
        "E0113",
    )
}

test "out_of_body_inherent_method_does_not_implicitly_satisfy_interface" {
    root.compiler.AssertHasCode(
        {
            "out_of_body_inherent_method_does_not_implicitly_satisfy_interface.baml": `
                interface Label {
                    function label(self, name: string) -> string throws never
                }

                class Thing {
                    function label(self, value: string) -> string {
                        return value
                    }
                }

                implements Label for Thing {}
            `,
        },
        "E0113",
    )
}

test "out_of_body_empty_implements_does_not_satisfy_abstract_via_inherent_method" {
    root.compiler.AssertHasCode(
        {
            "out_of_body_empty_implements_does_not_satisfy_abstract_via_inherent_method.baml": `
                interface Printable {
                    function label(self) -> string throws never
                }

                class Box {
                    value: string

                    function label(self) -> string {
                        return self.value
                    }
                }

                implements Printable for Box {}
            `,
        },
        "E0113",
    )
}

test "out_of_body_empty_implements_inherent_method_with_wrong_sig_does_not_satisfy" {
    root.compiler.AssertHasCode(
        {
            "out_of_body_empty_implements_inherent_method_with_wrong_sig_does_not_satisfy.baml": `
                interface Printable {
                    function label(self) -> string throws never
                }

                class Box {
                    function label(self) -> int {
                        return 1
                    }
                }

                implements Printable for Box {}
            `,
        },
        "E0113",
    )
}

test "out_of_body_implements_field_bearing_interface_is_error" {
    root.compiler.AssertHasCode(
        {
            "out_of_body_implements_field_bearing_interface_is_error.baml": `
                interface Named {
                    name: string
                    function greet(self) -> string throws never
                }
                class Robot { model: string }
                implements Named for Robot {
                    name: string
                    function greet(self) -> string { return "I am " + self.name }
                }
            `,
        },
        "E0126",
    )
}

test "out_of_body_implements_field_bearing_interface_is_error_even_without_redeclared_fields" {
    root.compiler.AssertHasCode(
        {
            "out_of_body_implements_field_bearing_interface_is_error_even_without_redeclared_fields.baml": `
                interface Named {
                    name: string
                    function greet(self) -> string throws never
                }
                class Robot { model: string }
                implements Named for Robot {
                    function greet(self) -> string { return "I am a robot" }
                }
            `,
        },
        "E0126",
    )
}

test "out_of_body_implements_inherited_field_bearing_interface_requires_parent" {
    root.compiler.AssertHasCode(
        {
            "out_of_body_implements_inherited_field_bearing_interface_requires_parent.baml": `
                interface Named {
                    name: string
                }
                interface Child requires Named {}
                class Robot { model: string }

                implements Child for Robot {}
            `,
        },
        "E0125",
    )
}

test "out_of_body_and_in_body_for_same_interface_is_error" {
    root.compiler.AssertHasCode(
        {
            "out_of_body_and_in_body_for_same_interface_is_error.baml": `
                interface ToJson {
                    function to_json(self) -> string throws never
                }
                class Dog {
                    implements ToJson {
                        function to_json(self) -> string { return "woof" }
                    }
                }
                implements ToJson for Dog {
                    function to_json(self) -> string { return "bark" }
                }
            `,
        },
        "E0132",
    )
}

test "out_of_body_implements_for_generic_function_type_is_rejected" {
    root.compiler.AssertHasMessage(
        {
            "out_of_body_implements_for_generic_function_type_is_rejected.baml": `
                interface GenericCallable {}
                implements GenericCallable for <T>(x: int) -> int {}
            `,
        },
        "generic parameters",
    )
}

test "generic_required_parent_args_must_match" {
    root.compiler.AssertHasCode(
        {
            "generic_required_parent_args_must_match.baml": `
                interface Parent<T> {}
                interface Child<T> requires Parent<T> {}

                class Box {
                    implements Parent<string> {}
                    implements Child<int> {}
                }
            `,
        },
        "E0125",
    )
}

test "out_of_body_generic_required_parent_args_must_match" {
    root.compiler.AssertHasCode(
        {
            "out_of_body_generic_required_parent_args_must_match.baml": `
                interface Parent<T> {}
                interface Child<T> requires Parent<T> {}

                implements Parent<string> for int {}
                implements Child<int> for int {}
            `,
        },
        "E0125",
    )
}

test "inherited_generic_interface_field_construction_uses_parent_args" {
    root.compiler.AssertHasMessage(
        {
            "inherited_generic_interface_field_construction_uses_parent_args.baml": `
                interface Parent<T> {
                    value: T
                }
                interface Child<T> requires Parent<T> {}
                class Box {
                    stored: int
                    implements Child<int> {}
                    implements Parent<int> {
                        value as stored
                    }
                }
                function main() -> void {
                    let b = Box { value: "wrong" }
                }
            `,
        },
        "expected `int`, found",
    )
}

test "bare_interface_method_value_requires_inferable_self" {
    root.compiler.AssertHasCode(
        {
            "bare_interface_method_value_requires_inferable_self.baml": `
                interface Label<T> {
                    function label(self) -> string throws never {
                        return "ok"
                    }
                }
                class Box {
                    implements Label<int> {}
                }
                function main() -> void {
                    let label = Label.label
                }
            `,
        },
        "E0002",
    )
}

test "out_of_body_implements_for_primitive_cannot_add_fields" {
    root.compiler.AssertHasMessage(
        {
            "out_of_body_implements_for_primitive_cannot_add_fields.baml": `
                interface Named {
                    name: string
                }
                implements Named for int {
                    name: string
                }
            `,
        },
        "field",
    )
}

test "cross_interface_assignment_is_compile_error" {
    root.compiler.AssertHasMessage(
        {
            "cross_interface_assignment_is_compile_error.baml": `
                interface Animal {
                    function speak(self) -> string throws never
                }
                interface Swimmer {
                    function swim(self) -> string throws never
                }
                class Duck {
                    implements Animal {
                        function speak(self) -> string { return "Quack!" }
                    }
                    implements Swimmer {
                        function swim(self) -> string { return "splash" }
                    }
                }
                function bad() -> string {
                    let a: Animal = Duck {}
                    let s: Swimmer = a
                    return s.swim()
                }
            `,
        },
        "Animal",
    )
}

test "implements_for_union_target_is_rejected" {
    root.compiler.AssertHasCode(
        {
            "implements_for_union_target_is_rejected.baml": `
                interface Tag {
                    function tag(self) -> int throws never
                }
                implements Tag for int | string {
                    function tag(self) -> int { return 0 }
                }
            `,
        },
        "E0138",
    )
}

test "implements_for_interface_target_is_rejected" {
    root.compiler.AssertHasCode(
        {
            "implements_for_interface_target_is_rejected.baml": `
                interface Tag {
                    function tag(self) -> int throws never
                }
                interface Other {
                    function other(self) -> int throws never
                }
                implements Tag for Other {
                    function tag(self) -> int { return 0 }
                }
            `,
        },
        "E0138",
    )
}

test "implements_for_optional_target_is_rejected" {
    root.compiler.AssertHasCode(
        {
            "implements_for_optional_target_is_rejected.baml": `
                interface Label {
                    function label(self) -> string throws never
                }
                implements<T> Label for T? {
                    function label(self) -> string { return "optional" }
                }
            `,
        },
        "E0138",
    )
}

test "impl_generic_bound_must_be_an_interface" {
    root.compiler.AssertHasCode(
        {
            "impl_generic_bound_must_be_an_interface.baml": `
                interface Printable {
                    function print(self) -> string throws never
                }
                class Widget {
                    name: string
                }
                implements<T extends Widget> Printable for T {
                    function print(self) -> string { return "w" }
                }
            `,
        },
        "E0145",
    )
}

test "implements_for_unknown_target_is_rejected" {
    root.compiler.AssertHasCode(
        {
            "implements_for_unknown_target_is_rejected.baml": `
                interface Tag {
                    function tag(self) -> int throws never
                }
                implements Tag for unknown {
                    function tag(self) -> int { return 0 }
                }
            `,
        },
        "E0138",
    )
}

test "implements_for_literal_target_is_rejected" {
    root.compiler.AssertHasCode(
        {
            "implements_for_literal_target_is_rejected.baml": `
                interface Tag {
                    function tag(self) -> int throws never
                }
                implement Tag for 1 {
                    function tag(self) -> int { return 0 }
                }
            `,
        },
        "E0138",
    )
}

test "implements_for_enum_variant_target_is_rejected" {
    root.compiler.AssertHasCode(
        {
            "implements_for_enum_variant_target_is_rejected.baml": `
                enum Color { Red, Green, Blue }
                interface Tag {
                    function tag(self) -> int throws never
                }
                implement Tag for Color.Red {
                    function tag(self) -> int { return 0 }
                }
            `,
        },
        "E0138",
    )
}

test "generic_rule_for_list_receiver_overlaps_concrete_list" {
    root.compiler.AssertHasCode(
        {
            "generic_rule_for_list_receiver_overlaps_concrete_list.baml": `
                interface Label {
                    function label(self) -> string throws never
                }
                implements Label for int[] {
                    function label(self) -> string { return "ints" }
                }
                implements<T> Label for T[] {
                    function label(self) -> string { return "list" }
                }
            `,
        },
        "E0132",
    )
}

test "unified_rule_rejects_mismatched_generic_interface_arg" {
    root.compiler.AssertHasMessage(
        {
            "unified_rule_rejects_mismatched_generic_interface_arg.baml": `
                interface Container<T> {
                    function get(self) -> T throws never
                }
                class Wrapper<T> {
                    value: T
                }
                implements<T> Container<T> for Wrapper<T> {
                    function get(self) -> T { return self.value }
                }
                function take(c: Container<string>) -> string {
                    return c.get()
                }
                function bad() -> string {
                    return take(Wrapper<int> { value: 42 })
                }
            `,
        },
        "Wrapper<int>",
    )
}

test "unified_rule_repeated_type_vars_reject_conflicting_args" {
    root.compiler.AssertHasMessage(
        {
            "unified_rule_repeated_type_vars_reject_conflicting_args.baml": `
                interface Same {
                    function tag(self) -> string throws never
                }
                class Pair<L, R> {
                    left: L
                    right: R
                }
                implements<T> Same for Pair<T, T> {
                    function tag(self) -> string { return "same" }
                }
                function take(p: Same) -> string {
                    return p.tag()
                }
                function bad() -> string {
                    return take(Pair<int, string> { left: 7, right: "nope" })
                }
            `,
        },
        "Pair<int, string>",
    )
}

test "form2_does_not_apply_when_bound_not_satisfied" {
    root.compiler.AssertHasMessage(
        {
            "form2_does_not_apply_when_bound_not_satisfied.baml": `
                interface Named {
                    name: string
                }
                interface Printable {
                    function display(self) -> string throws never
                }
                class Rock {
                    label: string
                }
                implements<T extends Named> Printable for T {
                    function display(self) -> string { return self.name }
                }
                function bad() -> string {
                    let item: Printable = Rock { label: "igneous" }
                    return item.display()
                }
            `,
        },
        "Rock",
    )
}

test "bounded_generic_function_type_annotation_is_rejected" {
    root.compiler.AssertHasMessage(
        {
            "bounded_generic_function_type_annotation_is_rejected.baml": `
                interface MyInterface {
                    function myMethod(self) -> int throws never
                }
                function main() -> void {
                    let method : <T extends MyInterface>(T) -> int = MyInterface.myMethod
                }
            `,
        },
        "generic parameters",
    )
}

test "parenthesized_generic_apply_is_rejected" {
    root.compiler.AssertHasMessage(
        {
            "parenthesized_generic_apply_is_rejected.baml": `
                function identity<T>(x: T) -> T { x }
                function caller() -> string {
                    let f = (identity)<int>
                    return "ok"
                }
            `,
        },
        "function reference",
    )
}

test "inferred_interface_method_reference_rejects_receiver_outside_bound" {
    root.compiler.AssertHasMessage(
        {
            "inferred_interface_method_reference_rejects_receiver_outside_bound.baml": `
                interface MyInterface {
                    function myMethod(self) -> int throws never
                }
                class Other {}
                function main() -> int {
                    let method = MyInterface.myMethod
                    return method(Other {})
                }
            `,
        },
        "MyInterface",
    )
}

test "overlapping_concrete_and_generic_rules_are_e0132" {
    root.compiler.AssertHasCode(
        {
            "overlapping_concrete_and_generic_rules_are_e0132.baml": `
                interface Printable {
                    function display(self) -> string throws never
                }
                class Box<T> {
                    value: T
                }
                implements<T> Printable for Box<T> {
                    function display(self) -> string { return "box" }
                }
                implements Printable for Box<int> {
                    function display(self) -> string { return "int box" }
                }
            `,
        },
        "E0132",
    )
}

test "overlapping_generic_rules_are_e0132" {
    root.compiler.AssertHasCode(
        {
            "overlapping_generic_rules_are_e0132.baml": `
                interface Printable {
                    function display(self) -> string throws never
                }
                class Box<T> {
                    value: T
                }
                implements<T> Printable for Box<T> {
                    function display(self) -> string { return "box" }
                }
                implements<U> Printable for Box<U> {
                    function display(self) -> string { return "other box" }
                }
            `,
        },
        "E0132",
    )
}

test "overlapping_complementary_generic_args_are_e0132" {
    root.compiler.AssertHasCode(
        {
            "overlapping_complementary_generic_args_are_e0132.baml": `
                interface Printable {
                    function display(self) -> string throws never
                }
                class Pair<A, B> {
                    first: A
                    second: B
                }
                implements<T> Printable for Pair<T, int> {
                    function display(self) -> string { return "t,int" }
                }
                implements<U> Printable for Pair<string, U> {
                    function display(self) -> string { return "string,u" }
                }
            `,
        },
        "E0132",
    )
}

test "overlapping_in_body_and_out_of_body_generic_rules_are_e0132" {
    root.compiler.AssertHasCode(
        {
            "overlapping_in_body_and_out_of_body_generic_rules_are_e0132.baml": `
                interface Printable {
                    function display(self) -> string throws never
                }
                class Box<T> {
                    value: T
                    implements Printable {
                        function display(self) -> string { return "in body" }
                    }
                }
                implements<T> Printable for Box<T> {
                    function display(self) -> string { return "out of body" }
                }
            `,
        },
        "E0132",
    )
}

test "cross_file_overlapping_impls_are_e0132" {
    root.compiler.AssertHasCode(
        {
            "a.baml": `
                interface Printable {
                    function display(self) -> string throws never
                }
                class Box<T> { value: T }
                implements<T> Printable for Box<T> {
                    function display(self) -> string { return "box" }
                }
            `,
            "b.baml": `
                implements Printable for Box<int> {
                    function display(self) -> string { return "int box" }
                }
            `,
        },
        "E0132",
    )
}

test "form2_overlap_with_form2_is_e0132" {
    root.compiler.AssertHasCode(
        {
            "form2_overlap_with_form2_is_e0132.baml": `
                interface Named {
                    name: string
                }
                interface Printable {
                    function display(self) -> string throws never
                }
                implements<T extends Named> Printable for T {
                    function display(self) -> string { return "first" }
                }
                implements<U extends Named> Printable for U {
                    function display(self) -> string { return "second" }
                }
            `,
        },
        "E0132",
    )
}

test "overlapping_bounded_generic_receiver_rules_are_e0132" {
    root.compiler.AssertHasCode(
        {
            "overlapping_bounded_generic_receiver_rules_are_e0132.baml": `
                interface Named {
                    name: string
                }
                interface Tagged {
                    tag: string
                }
                interface Printable {
                    function display(self) -> string throws never
                }
                class Box<T> {
                    value: T
                }
                implements<T extends Named> Printable for Box<T> {
                    function display(self) -> string { return "named box" }
                }
                implements<U extends Tagged> Printable for Box<U> {
                    function display(self) -> string { return "tagged box" }
                }
            `,
        },
        "E0132",
    )
}

test "overlapping_bounded_and_unbounded_generic_receiver_rules_are_e0132" {
    root.compiler.AssertHasCode(
        {
            "overlapping_bounded_and_unbounded_generic_receiver_rules_are_e0132.baml": `
                interface Named {
                    name: string
                }
                interface Printable {
                    function display(self) -> string throws never
                }
                class Box<T> {
                    value: T
                }
                implements<T> Printable for Box<T> {
                    function display(self) -> string { return "box" }
                }
                implements<U extends Named> Printable for Box<U> {
                    function display(self) -> string { return "named box" }
                }
            `,
        },
        "E0132",
    )
}

test "bounded_type_var_rule_overlaps_concrete_satisfying_bound" {
    root.compiler.AssertHasCode(
        {
            "bounded_type_var_rule_overlaps_concrete_satisfying_bound.baml": `
                interface Named {
                    name: string
                }
                interface Printable {
                    function display(self) -> string throws never
                }
                class User {
                    name: string
                    implements Named {}
                }
                implements Printable for User {
                    function display(self) -> string { return "user" }
                }
                implements<T extends Named> Printable for T {
                    function display(self) -> string { return "named" }
                }
            `,
        },
        "E0132",
    )
}

test "distinct_bounded_blankets_overlap_e0132" {
    root.compiler.AssertHasCode(
        {
            "distinct_bounded_blankets_overlap_e0132.baml": `
                interface A { a: int }
                interface B { b: int }
                interface Printable { function display(self) -> string throws never }
                implements<T extends A> Printable for T {
                    function display(self) -> string { return "a" }
                }
                implements<T extends B> Printable for T {
                    function display(self) -> string { return "b" }
                }
            `,
        },
        "E0132",
    )
}

test "unbounded_blanket_overlaps_concrete_e0132" {
    root.compiler.AssertHasCode(
        {
            "unbounded_blanket_overlaps_concrete_e0132.baml": `
                interface Printable { function display(self) -> string throws never }
                class Widget {}
                implements<T> Printable for T {
                    function display(self) -> string { return "any" }
                }
                implements Printable for Widget {
                    function display(self) -> string { return "widget" }
                }
            `,
        },
        "E0132",
    )
}

test "orphan_rejects_foreign_interface_for_local_array" {
    root.compiler.AssertHasCode(
        {
            "orphan_rejects_foreign_interface_for_local_array.baml": `
                class Local {}
                implement baml.ops.Equals for Local[] {}
            `,
        },
        "E0139",
    )
}

test "orphan_rejects_foreign_interface_for_map_of_local" {
    root.compiler.AssertHasCode(
        {
            "orphan_rejects_foreign_interface_for_map_of_local.baml": `
                class Local {}
                implement baml.ops.Equals for map<string, Local> {}
            `,
        },
        "E0139",
    )
}

test "shared_var_across_interface_arg_and_for_type_overlaps_e0132" {
    root.compiler.AssertHasCode(
        {
            "shared_var_across_interface_arg_and_for_type_overlaps_e0132.baml": `
                interface P<X> { function p(self) -> string throws never }
                class Box<T> { v: T }
                implements<T> P<T> for Box<T> {
                    function p(self) -> string { return "t" }
                }
                implements<U> P<int> for Box<U> {
                    function p(self) -> string { return "int" }
                }
            `,
        },
        "E0132",
    )
}

test "variable_in_union_arg_overlaps_via_union_expansion" {
    root.compiler.AssertHasMessage(
        {
            "variable_in_union_arg_overlaps_via_union_expansion.baml": `
                interface Marker {}
                class Box<X> { v: X }
                implements<T> Marker for Box<int | T> {}
                implements Marker for Box<int | string | bool> {}
            `,
        },
        "overlapping interface implementations",
    )
}

test "definite_overlap_reports_overlapping_message" {
    root.compiler.AssertHasMessage(
        {
            "definite_overlap_reports_overlapping_message.baml": `
                interface Marker {}
                class Widget {}
                implements<T> Marker for T {}
                implements Marker for Widget {}
            `,
        },
        "overlapping interface implementations",
    )
}

test "nested_generic_complementary_args_overlap_e0132" {
    root.compiler.AssertHasCode(
        {
            "nested_generic_complementary_args_overlap_e0132.baml": `
                interface Marker {}
                class Box<X> { v: X }
                class Pair<A, B> { first: A second: B }
                implements<T> Marker for Box<Pair<T, int>> {}
                implements<U> Marker for Box<Pair<string, U>> {}
            `,
        },
        "E0132",
    )
}

test "nested_union_in_generic_arg_overlaps_via_expansion" {
    root.compiler.AssertHasCode(
        {
            "nested_union_in_generic_arg_overlaps_via_expansion.baml": `
                interface Marker {}
                class Box<X> { v: X }
                class Pair<A, B> { first: A second: B }
                implements<T> Marker for Box<Pair<int, string | T>> {}
                implements Marker for Box<Pair<int, string | bool>> {}
            `,
        },
        "E0132",
    )
}

test "deeply_nested_generic_with_list_overlaps_e0132" {
    root.compiler.AssertHasCode(
        {
            "deeply_nested_generic_with_list_overlaps_e0132.baml": `
                interface Marker {}
                class Box<X> { v: X }
                class Pair<A, B> { first: A second: B }
                implements<T> Marker for Box<Pair<T[], int>> {}
                implements<U> Marker for Box<Pair<string[], U>> {}
            `,
        },
        "E0132",
    )
}

test "union_collapse_overlaps_e0132" {
    root.compiler.AssertHasCode(
        {
            "union_collapse_overlaps_e0132.baml": `
                interface Marker {}
                class Box<X> { v: X }
                class Foo { x: int }
                class Bar { y: int }
                implements<T, U, W> Marker for Box<T[] | U[] | W[]> {}
                implements Marker for Box<Foo[] | Bar[]> {}
            `,
        },
        "E0132",
    )
}

test "complete_bool_union_folds_and_overlaps_e0132" {
    root.compiler.AssertHasCode(
        {
            "complete_bool_union_folds_and_overlaps_e0132.baml": `
                interface Marker {}
                class Box<X> { v: X }
                implements Marker for Box<true | false> {}
                implements Marker for Box<bool> {}
            `,
        },
        "E0132",
    )
}

test "complete_enum_union_folds_and_overlaps_e0132" {
    root.compiler.AssertHasCode(
        {
            "complete_enum_union_folds_and_overlaps_e0132.baml": `
                interface Marker {}
                class Box<X> { v: X }
                enum Cmp { Less Equal More }
                implements Marker for Box<Cmp.Less | Cmp.Equal | Cmp.More> {}
                implements Marker for Box<Cmp> {}
            `,
        },
        "E0132",
    )
}

test "intractable_union_overlap_asks_to_simplify" {
    root.compiler.AssertHasMessage(
        {
            "intractable_union_overlap_asks_to_simplify.baml": `
                interface Marker {}
                class Box<X> { v: X }
                class Pair<A, B> { f: A s: B }
                class A0 { x: int } class A1 { x: int } class A2 { x: int }
                class A3 { x: int } class A4 { x: int } class A5 { x: int }
                implements<T0, T1, T2, T3, T4> Marker for Box<
                    Pair<T0, T0> | Pair<T1, T1> | Pair<T2, T2> | Pair<T3, T3> | Pair<T4, T4>
                > {}
                implements Marker for Box<
                    Pair<A0, A0> | Pair<A1, A1> | Pair<A2, A2> | Pair<A3, A3> | Pair<A4, A4> | Pair<A5, A5>
                > {}
            `,
        },
        "simplify",
    )
}

test "union_member_vs_single_collapse_overlaps_e0132" {
    root.compiler.AssertHasCode(
        {
            "union_member_vs_single_collapse_overlaps_e0132.baml": `
                interface Marker {}
                class Box<X> { v: X }
                class C {}
                implements<T> Marker for Box<C | T> {}
                implements Marker for Box<C> {}
            `,
        },
        "E0132",
    )
}

test "box_unknown_vs_blanket_overlaps_e0132" {
    root.compiler.AssertHasCode(
        {
            "box_unknown_vs_blanket_overlaps_e0132.baml": `
                interface Marker {}
                class Box<X> { v: X }
                implements<T> Marker for Box<T> {}
                implements Marker for Box<unknown> {}
            `,
        },
        "E0132",
    )
}

test "bounded_union_member_overlaps_concrete_union_e0132" {
    root.compiler.AssertHasCode(
        {
            "bounded_union_member_overlaps_concrete_union_e0132.baml": `
                interface HasName {}
                interface Marker {}
                class Box<X> { v: X }
                class Cat {}
                class Dog {}
                implements HasName for Dog {}
                implements<T extends HasName, R> Marker for Box<T[] | R> {}
                implements Marker for Box<Cat[] | Dog[]> {}
            `,
        },
        "E0132",
    )
}

test "bounded_union_member_overlaps_concrete_union_reversed_e0132" {
    root.compiler.AssertHasCode(
        {
            "bounded_union_member_overlaps_concrete_union_reversed_e0132.baml": `
                interface HasName {}
                interface Marker {}
                class Box<X> { v: X }
                class Cat {}
                class Dog {}
                implements HasName for Dog {}
                implements<T extends HasName, R> Marker for Box<T[] | R> {}
                implements Marker for Box<Dog[] | Cat[]> {}
            `,
        },
        "E0132",
    )
}

test "alias_for_type_overlaps_concrete_e0132" {
    root.compiler.AssertHasCode(
        {
            "alias_for_type_overlaps_concrete_e0132.baml": `
                interface Marker {}
                class C {}
                type AliasC = C
                implement Marker for C {}
                implement Marker for AliasC {}
            `,
        },
        "E0132",
    )
}

test "alias_of_alias_for_type_overlaps_e0132" {
    root.compiler.AssertHasCode(
        {
            "alias_of_alias_for_type_overlaps_e0132.baml": `
                interface Marker {}
                class C {}
                type A = C
                type B = A
                implement Marker for C {}
                implement Marker for B {}
            `,
        },
        "E0132",
    )
}

test "two_distinct_aliases_of_same_class_overlap_e0132" {
    root.compiler.AssertHasCode(
        {
            "two_distinct_aliases_of_same_class_overlap_e0132.baml": `
                interface Marker {}
                class C {}
                type A = C
                type B = C
                implement Marker for A {}
                implement Marker for B {}
            `,
        },
        "E0132",
    )
}

test "alias_for_type_overlaps_blanket_e0132" {
    root.compiler.AssertHasCode(
        {
            "alias_for_type_overlaps_blanket_e0132.baml": `
                interface Marker {}
                class Box<X> { v: X }
                type BI = Box<int>
                implements<T> Marker for Box<T> {}
                implement Marker for BI {}
            `,
        },
        "E0132",
    )
}

test "future_for_type_is_rejected_e0138" {
    root.compiler.AssertHasCode(
        {
            "future_for_type_is_rejected_e0138.baml": `
                interface Marker {}
                implement Marker for baml.future.Future<int, string> {}
            `,
        },
        "E0138",
    )
}

test "bounded_type_var_rule_conservatively_overlaps_generic_class_rule" {
    root.compiler.AssertHasCode(
        {
            "bounded_type_var_rule_conservatively_overlaps_generic_class_rule.baml": `
                interface Named {
                    name: string
                }
                interface Printable {
                    function display(self) -> string throws never
                }
                class Box<T> {
                    value: T
                }
                implements<T> Printable for Box<T> {
                    function display(self) -> string { return "box" }
                }
                implements<U extends Named> Printable for U {
                    function display(self) -> string { return "named" }
                }
            `,
        },
        "E0132",
    )
}

test "bounded_type_var_rule_conservatively_overlaps_in_body_generic_class_rule" {
    root.compiler.AssertHasCode(
        {
            "bounded_type_var_rule_conservatively_overlaps_in_body_generic_class_rule.baml": `
                interface Named {
                    name: string
                }
                interface Printable {
                    function display(self) -> string throws never
                }
                class Box<T> {
                    value: T
                    implements Printable {
                        function display(self) -> string { return "box" }
                    }
                }
                implements<U extends Named> Printable for U {
                    function display(self) -> string { return "named" }
                }
            `,
        },
        "E0132",
    )
}

test "fuzz_bug06_interface_requires_non_interface_errors_at_declaration" {
    root.compiler.AssertHasMessage(
        {
            "fuzz_bug06_interface_requires_non_interface_errors_at_declaration.baml": `
                class Foo { x: int }
                interface Bad requires Foo {}
                function main() -> bool { return true }
            `,
        },
        "Foo",
    )
}

test "fuzz_bug08_generic_class_overlapping_type_arg_impls_rejected" {
    root.compiler.AssertHasCode(
        {
            "fuzz_bug08_generic_class_overlapping_type_arg_impls_rejected.baml": `
                interface Getter<T> {
                    function get(self) -> T throws never
                }

                class Pair<L, R> {
                    left: L
                    right: R
                    implements Getter<L> {
                        function get(self) -> L { return self.left }
                    }
                    implements Getter<R> {
                        function get(self) -> R { return self.right }
                    }
                }

                function main() -> bool {
                    let p: Pair<int, string> = Pair { left: 7, right: "seven" }
                    let gr: Getter<string> = p
                    let val = gr.get()
                    return val == "seven"
                }
            `,
        },
        "E0132",
    )
}

test "fuzz_bug09_same_generic_iface_diff_typeargs_unqualified_call_ambiguous" {
    root.compiler.AssertHasCode(
        {
            "fuzz_bug09_same_generic_iface_diff_typeargs_unqualified_call_ambiguous.baml": `
                interface Converter<T> {
                    function convert(self) -> T throws never
                }

                class MultiFormat {
                    int_val: int
                    str_val: string
                    implements Converter<int> {
                        function convert(self) -> int { return self.int_val }
                    }
                    implements Converter<string> {
                        function convert(self) -> string { return self.str_val }
                    }
                }

                function main() -> int {
                    let m = MultiFormat { int_val: 42, str_val: "hello" }
                    return m.convert()
                }
            `,
        },
        "E0121",
    )
}

test "fuzz_bug11_ambiguous_generic_field_error_includes_type_args" {
    root.compiler.AssertHasMessage(
        {
            "fuzz_bug11_ambiguous_generic_field_error_includes_type_args.baml": `
                interface Box<T> {
                    value: T
                }

                class MultiBox {
                    int_v: int
                    str_v: string
                    implements Box<int> { value as int_v }
                    implements Box<string> { value as str_v }
                }

                function main() -> int {
                    let b = MultiBox { int_v: 42, str_v: "hello" }
                    return b.value
                }
            `,
        },
        "as<Box<int>>",
    )
}

test "fuzz_bug12_generic_pair_overlapping_type_arg_impls_rejected" {
    root.compiler.AssertHasCode(
        {
            "fuzz_bug12_generic_pair_overlapping_type_arg_impls_rejected.baml": `
                interface Slot<T> {
                    function get(self) -> T throws never
                }

                class GenPair<L, R> {
                    left: L
                    right: R
                    implements Slot<L> {
                        function get(self) -> L { return self.left }
                    }
                    implements Slot<R> {
                        function get(self) -> R { return self.right }
                    }
                }

                function main() -> bool {
                    let p: GenPair<int, string> = GenPair { left: 42, right: "world" }
                    let rl: Slot<int> = p
                    let rr: Slot<string> = p
                    let lv = rl.get()
                    let rv = rr.get()
                    // Each interface view dispatches to its own impl block:
                    // lv == 42 (Slot<int>) && rv == "world" (Slot<string>).
                    return lv == 42 && rv == "world"
                }
            `,
        },
        "E0132",
    )
}

test "fuzz_bug13_generic_field_link_overlapping_type_arg_impls_rejected" {
    root.compiler.AssertHasCode(
        {
            "fuzz_bug13_generic_field_link_overlapping_type_arg_impls_rejected.baml": `
                interface Slot<T> {
                    value: T
                }

                class GenPair<L, R> {
                    left: L
                    right: R
                    implements Slot<L> {
                        value as left
                    }
                    implements Slot<R> {
                        value as right
                    }
                }

                function main() -> bool {
                    let p: GenPair<int, string> = GenPair { left: 7, right: "seven" }
                    let i: Slot<int> = p
                    let s: Slot<string> = p
                    // Each field-link view selects the impl block matching its type arg:
                    // i.value == 7 (Slot<int>) && s.value == "seven" (Slot<string>).
                    return i.value == 7 && s.value == "seven"
                }
            `,
        },
        "E0132",
    )
}

test "fuzz_bug14_generic_overlapping_type_arg_impls_reject_as_projection" {
    root.compiler.AssertHasCode(
        {
            "fuzz_bug14_generic_overlapping_type_arg_impls_reject_as_projection.baml": `
                interface Slot<T> {
                    function get(self) -> T throws never
                }

                class GenPair<L, R> {
                    left: L
                    right: R
                    implements Slot<L> {
                        function get(self) -> L { return self.left }
                    }
                    implements Slot<R> {
                        function get(self) -> R { return self.right }
                    }
                }

                function main() -> bool {
                    let p: GenPair<int, string> = GenPair { left: 42, right: "world" }
                    // Explicit .as<> projection should select the Slot<string> block
                    let rv = p.as<Slot<string>>.get()
                    return rv == "world"
                }
            `,
        },
        "E0132",
    )
}

test "fuzz_bug19_aliased_field_type_mismatch_error_names_interface_field" {
    root.compiler.AssertHasMessage(
        {
            "fuzz_bug19_aliased_field_type_mismatch_error_names_interface_field.baml": `
                interface Named {
                    name: string
                }
                class Person {
                    name_count: int
                    implements Named {
                        name as name_count
                    }
                }
                function main() -> string {
                    return "unreachable"
                }
            `,
        },
        "`name`",
    )
}

test "fuzz_bug22_old_projection_syntax_is_no_member_error" {
    root.compiler.AssertHasMessage(
        {
            "fuzz_bug22_old_projection_syntax_is_no_member_error.baml": `
                interface Container<T> {
                    function get(self) -> T throws never
                }
                class IntBox {
                    value: int
                    implements Container<int> {
                        function get(self) -> int { return self.value }
                    }
                }
                function main() -> int {
                    let b = IntBox { value: 42 }
                    return b.Container.get()
                }
            `,
        },
        "has no member `Container`",
    )
}

test "fuzz_bug25_two_same_named_default_methods_are_ambiguous" {
    root.compiler.AssertHasCode(
        {
            "fuzz_bug25_two_same_named_default_methods_are_ambiguous.baml": `
                interface Alpha {
                    function tag(self) -> string throws never { return "alpha" }
                }
                interface Beta {
                    function tag(self) -> string throws never { return "beta" }
                }

                class Both {
                    implements Alpha {}
                    implements Beta {}
                }

                function main() -> string {
                    let x = Both {}
                    return x.tag()
                }
            `,
        },
        "E0121",
    )
}

test "fuzz_bug26_default_plus_required_same_name_is_ambiguous" {
    root.compiler.AssertHasCode(
        {
            "fuzz_bug26_default_plus_required_same_name_is_ambiguous.baml": `
                interface WithDefault {
                    function process(self) -> string throws never { return "DEFAULT" }
                }
                interface WithRequired {
                    function process(self) -> string throws never
                }

                class Impl {
                    implements WithDefault {}
                    implements WithRequired {
                        function process(self) -> string { return "REQUIRED" }
                    }
                }

                function main() -> string {
                    let x = Impl {}
                    return x.process()  // E0121: ambiguous between the default and required \`process\`
                }
            `,
        },
        "E0121",
    )
}

test "fuzz_bug27_same_generic_iface_diff_typeargs_no_receiver_is_ambiguous" {
    root.compiler.AssertHasCode(
        {
            "fuzz_bug27_same_generic_iface_diff_typeargs_no_receiver_is_ambiguous.baml": `
                interface Converter<T> {
                    function convert(self) -> T throws never
                }

                class Multi {
                    implements Converter<int> {
                        function convert(self) -> int { return 42 }
                    }
                    implements Converter<string> {
                        function convert(self) -> string { return "hello" }
                    }
                }

                function main() -> int {
                    let m = Multi {}
                    return m.convert()  // E0121: ambiguous between Converter<int> and Converter<string>
                }
            `,
        },
        "E0121",
    )
}

test "fuzz_bug28_ambiguous_generic_field_error_lists_type_args" {
    root.compiler.AssertHasMessage(
        {
            "fuzz_bug28_ambiguous_generic_field_error_lists_type_args.baml": `
                interface Slot<T> {
                    value: T
                }

                class Pair {
                    int_value: int
                    str_value: string
                    implements Slot<int> { value as int_value }
                    implements Slot<string> { value as str_value }
                }

                function main() -> string {
                    let p = Pair { int_value: 1, str_value: "hi" }
                    return p.value
                }
            `,
        },
        "Slot<int>",
    )
}

test "requires_real_non_interface_is_still_non_interface_error" {
    root.compiler.AssertHasCode(
        {
            "requires_real_non_interface_is_still_non_interface_error.baml": `
                class RealClass { x: int }
                interface Person requires RealClass {
                    name: string
                }
            `,
        },
        "E0133",
    )
}

test "wf3_generic_default_method_overlapping_type_arg_impls_rejected" {
    root.compiler.AssertHasCode(
        {
            "wf3_generic_default_method_overlapping_type_arg_impls_rejected.baml": `
                interface Slot<T> {
                    function get(self) -> T throws never
                    function describe(self) -> T throws never {
                        return self.get()
                    }
                }
                class GenPair<L, R> {
                    left: L
                    right: R
                    implements Slot<L> {
                        function get(self) -> L { return self.left }
                    }
                    implements Slot<R> {
                        function get(self) -> R { return self.right }
                    }
                }
                function main() -> string {
                    let p: GenPair<int, string> = GenPair { left: 7, right: "seven" }
                    let s: Slot<string> = p
                    return s.describe()
                }
            `,
        },
        "E0132",
    )
}

test "wf3_monomorph_collision_overlapping_impls_rejected" {
    root.compiler.AssertHasMessage(
        {
            "wf3_monomorph_collision_overlapping_impls_rejected.baml": `
                interface Getter<T> {
                    function get(self) -> T throws never
                }
                class Pair<L, R> {
                    left: L
                    right: R
                    implements Getter<L> {
                        function get(self) -> L { return self.left }
                    }
                    implements Getter<R> {
                        function get(self) -> R { return self.right }
                    }
                }
                function main() -> int {
                    let p: Pair<int, int> = Pair { left: 7, right: 99 }
                    return p.get()
                }
            `,
        },
        "overlapping interface implementations",
    )
}

test "wf3_unsatisfied_blanket_bound_message_names_bound" {
    root.compiler.AssertHasMessage(
        {
            "wf3_unsatisfied_blanket_bound_message_names_bound.baml": `
                interface Named { name: string }
                interface Printable { function display(self) -> string throws never }
                class Rock { label: string }
                class Box<T> { value: T }
                implements<T extends Named> Printable for Box<T> {
                    function display(self) -> string { return self.value.name }
                }
                function main() -> string {
                    let item: Printable = Box<Rock> { value: Rock { label: "ig" } }
                    return item.display()
                }
            `,
        },
        "Named",
    )
}

test "wf3_as_to_unimplemented_interface_message_mentions_implement" {
    root.compiler.AssertHasMessage(
        {
            "wf3_as_to_unimplemented_interface_message_mentions_implement.baml": `
                interface Animal {
                    function speak(self) -> string throws never
                }
                interface Vehicle {
                    function drive(self) -> string throws never
                }
                class Cat {
                    implements Animal {
                        function speak(self) -> string { return "Meow." }
                    }
                }
                function main() -> string {
                    let c = Cat {}
                    return c.as<Vehicle>.drive()
                }
            `,
        },
        "implement",
    )
}

test "wf3_default_as_bare_value_is_rejected" {
    root.compiler.AssertHasMessage(
        {
            "wf3_default_as_bare_value_is_rejected.baml": `
                interface Logger {
                    function log(self, msg: string) -> string throws never { return msg }
                }
                class C {
                    implements Logger {
                        function log(self, msg: string) -> string {
                            let x = default
                            return msg
                        }
                    }
                }
                function main() -> string {
                    let c = C {}
                    return c.log("hi")
                }
            `,
        },
        "default",
    )
}

test "wf3_self_interface_field_access_in_method_requires_projection" {
    root.compiler.AssertHasMessage(
        {
            "wf3_self_interface_field_access_in_method_requires_projection.baml": `
                interface Named {
                    name: string
                    function greet(self) -> string throws never
                }
                class Person {
                    title: string
                    implements Named {
                        name as title
                        function greet(self) -> string {
                            return "Hi " + self.name
                        }
                    }
                }
            `,
        },
        "as<Named>",
    )
}

test "wf3_all_default_requires_parent_still_needs_explicit_implements" {
    root.compiler.AssertHasCode(
        {
            "wf3_all_default_requires_parent_still_needs_explicit_implements.baml": `
                interface A {
                    function greet(self) -> string throws never { return "hi" }
                }
                interface B requires A {
                    function bye(self) -> string throws never { return "bye" }
                }
                class C {
                    implements B {}
                }
                function main() -> string {
                    let c: B = C {}
                    return c.bye()
                }
            `,
        },
        "E0125",
    )
}

test "wf3_requires_cycle_reports_full_path" {
    root.compiler.AssertHasMessage(
        {
            "wf3_requires_cycle_reports_full_path.baml": `
                interface A requires B { function fa(self) -> string throws never }
                interface B requires A { function fb(self) -> string throws never }
                class C {
                    implements A { function fa(self) -> string { return "a" } }
                    implements B { function fb(self) -> string { return "b" } }
                }
                function main() -> string {
                    let c: A = C {}
                    return c.fa()
                }
            `,
        },
        "A → B",
    )
}

test "wf3_out_of_body_primitive_field_bearing_is_e0126_pins" {
    root.compiler.AssertHasCode(
        {
            "wf3_out_of_body_primitive_field_bearing_is_e0126_pins.baml": `
                interface Named {
                    name: string
                    function display(self) -> string throws never
                }
                implements Named for int {
                    function display(self) -> string { return "int" }
                }
                function main() -> string { return "ok" }
            `,
        },
        "E0126",
    )
}

test "union_fuzz_class_only_union_method_is_rejected" {
    root.compiler.AssertHasMessage(
        {
            "union_fuzz_class_only_union_method_is_rejected.baml": `
                class A {
                  name: string
                  function execute(self) -> string { return "A executes and gets " + self.name }
                }
                class B {
                  name: string
                  function execute(self) -> string { return "B executes and gets " + self.name }
                }
                function process(input: A | B) -> string { return input.execute() }
                function main() -> string {
                  return process(A { name: "Alice" }) + " | " + process(B { name: "Bob" })
                }
            `,
        },
        "no common interface that declares",
    )
}

test "union_ruling_mixed_inherent_and_impl_arm_is_rejected" {
    root.compiler.AssertHasMessage(
        {
            "union_ruling_mixed_inherent_and_impl_arm_is_rejected.baml": `
                interface HudSpeaker {
                    function speak(self) -> string throws never
                }
                class HudA {
                    implements HudSpeaker {
                        function speak(self) -> string { return "interface" }
                    }
                }
                class HudB {
                    function speak(self) -> string { return "inherent" }
                }
                function hud_speak(x: HudA | HudB) -> string {
                    return x.speak()
                }
                function main() -> string {
                    return hud_speak(HudB {})
                }
            `,
        },
        "no common interface that declares",
    )
}

test "same_head_impls_differing_only_in_bounds_are_rejected_as_overlapping" {
    root.compiler.AssertHasCode(
        {
            "same_head_impls_differing_only_in_bounds_are_rejected_as_overlapping.baml": `
                interface CbdMarkA {
                    function ma(self) -> int throws never
                }
                interface CbdMarkB {
                    function mb(self) -> int throws never
                }
                interface CbdConv {
                    function conv(self) -> int throws never
                }
                class CbdBox<T> {
                    inner: T
                }
                implements<T extends CbdMarkA> CbdConv for CbdBox<T> {
                    function conv(self) -> int throws never { return 1 }
                }
                implements<T extends CbdMarkB> CbdConv for CbdBox<T> {
                    function conv(self) -> int throws never { return 2 }
                }
            `,
        },
        "E0132",
    )
}

test "default_bypass_of_required_method_is_rejected" {
    root.compiler.AssertHasMessage(
        {
            "default_bypass_of_required_method_is_rejected.baml": `
                interface DreqIface {
                    function must(self) -> string throws never
                    function wrap(self) -> string throws never { "w" }
                }
                class DreqHolder {
                    implements DreqIface {
                        function must(self) -> string throws never { "m" }
                        function wrap(self) -> string throws never { "x " + default.must() }
                    }
                }
                function dreq_probe(h: DreqHolder) -> string throws never {
                    h.wrap()
                }
            `,
        },
        "unresolved name: `default.must`",
    )
}

test "union_fuzz_pr_shared_implementor_method_in_iface_union_has_no_common_interface" {
    root.compiler.AssertHasMessage(
        {
            "union_fuzz_pr_shared_implementor_method_in_iface_union_has_no_common_interface.baml": `
                interface A { function m(self) -> string  throws never { return "from-A" } }
                interface B { function m(self) -> string  throws never { return "from-B" } }
                class C { implements A {} implements B {} }
                function call(x: A | B) -> string { return x.m() }
                function main() -> string { let c: A = C {}; return call(c) }
            `,
        },
        "no common interface that declares",
    )
}

test "union_fuzz_pr_disjoint_implementors_iface_union_is_rejected" {
    root.compiler.AssertHasMessage(
        {
            "union_fuzz_pr_disjoint_implementors_iface_union_is_rejected.baml": `
                interface A { function m(self) -> string  throws never { return "a" } }
                interface B { function m(self) -> string  throws never { return "b" } }
                class Dog { implements A {} }
                class Cat { implements B {} }
                function call(x: A | B) -> string { return x.m() }
                function main() -> string {
                  let d: A = Dog {}
                  let c: B = Cat {}
                  return call(d) + call(c)
                }
            `,
        },
        "no common interface that declares",
    )
}

test "union_fuzz_f03_conflicting_union_field_has_no_common_interface" {
    root.compiler.AssertHasMessage(
        {
            "union_fuzz_f03_conflicting_union_field_has_no_common_interface.baml": `
                interface Animal { id: string }
                interface Vehicle { id: int }
                class Dog { id: string  implements Animal {} }
                class Car { id: int  implements Vehicle {} }
                function pick(b: bool) -> Animal | Vehicle {
                  if b { return Dog { id: "rex" } } else { return Car { id: 7 } }
                }
                function main() -> string {
                  let u: Animal | Vehicle = pick(false)
                  let v: string = u.id
                  return v
                }
            `,
        },
        "no common interface that declares",
    )
}

test "union_fuzz_f07_shared_method_on_iface_union_is_a_compile_error" {
    root.compiler.AssertHasMessage(
        {
            "union_fuzz_f07_shared_method_on_iface_union_is_a_compile_error.baml": `
                interface Animal {
                  function speak(self) -> string throws never
                }
                class Dog {
                  implements Animal {
                    function speak(self) -> string { return "Woof" }
                  }
                }
                interface Vehicle {
                  function speak(self) -> string throws never
                }
                class Car {
                  implements Vehicle {
                    function speak(self) -> string { return "Vroom" }
                  }
                }
                function main() -> string {
                  let v: Animal | Vehicle = Dog {};
                  return v.speak();
                }
            `,
        },
        "no common interface that declares",
    )
}

test "union_fuzz_f11_conflicting_union_field_read_is_rejected" {
    root.compiler.AssertHasMessage(
        {
            "union_fuzz_f11_conflicting_union_field_read_is_rejected.baml": `
                interface Animal { id: string }
                interface Vehicle { id: int }
                class Dog { id: string  implements Animal {} }
                class Car { id: int  implements Vehicle {} }
                function readBool(u: Animal | Vehicle) -> bool {
                  let v: bool = u.id
                  return v
                }
                function main() -> bool { return readBool(Dog { id: "x" }) }
            `,
        },
        "no common interface that declares",
    )
}

test "union_fuzz_f16_unqualified_ambiguous_union_field_is_e0131" {
    root.compiler.AssertHasCode(
        {
            "union_fuzz_f16_unqualified_ambiguous_union_field_is_e0131.baml": `
                interface Named { name: string }
                interface Labeled { name: string }
                class Person {
                  full: string
                  handle: string
                  implements Named { name as full }
                  implements Labeled { name as handle }
                }
                class Company {
                  legal: string
                  brand: string
                  implements Named { name as legal }
                  implements Labeled { name as brand }
                }
                function rawName(x: Person | Company) -> string {
                  return x.name
                }
                function main() -> string {
                  return rawName(Person { full: "Ada", handle: "ada99" })
                }
            `,
        },
        "E0131",
    )
}

test "union_fuzz_f17_projection_failure_names_full_generic_interface" {
    root.compiler.AssertHasMessage(
        {
            "union_fuzz_f17_projection_failure_names_full_generic_interface.baml": `
                interface Cargo<T> { payload: T }
                class Box<T> {
                  inner: T
                  implements Cargo<T> { payload as inner }
                }
                function takeInt(x: Box<int> | Box<string>) -> int {
                  return x.as<Cargo<int>>.payload
                }
                function main() -> int {
                  let b: Box<int> | Box<string> = Box<string> { inner: "oops" }
                  return takeInt(b)
                }
            `,
        },
        "Cargo<int>",
    )
}

test "field_chain_abstract_interface_method_arity_checked" {
    root.compiler.AssertHasMessage(
        {
            "field_chain_abstract_interface_method_arity_checked.baml": `
                interface Conv {
                    function convert<T>(self, v: T) -> T throws never
                }
                class Celsius {
                    tag: string
                    implements Conv {
                        function convert<T>(self, v: T) -> T { v }
                    }
                }
                class Holder {
                    c: Conv
                }
                function main() -> int {
                    let h = Holder { c: Celsius { tag: "x" } };
                    h.c.convert<int, string>(5)
                }
            `,
        },
        "type argument",
    )
}

test "duplicate_implements_differing_only_in_assoc_bindings_is_compile_error" {
    root.compiler.AssertHasCode(
        {
            "duplicate_implements_differing_only_in_assoc_bindings_is_compile_error.baml": `
                interface Sink {
                    type Error
                    function push(self, v: int) -> string throws never
                }

                class Buf<E1, E2> {
                    tag: string
                    implements Sink {
                        type Error = E1
                        function push(self, v: int) -> string { "a" }
                    }
                    implements Sink {
                        type Error = E2
                        function push(self, v: int) -> string { "b" }
                    }
                }
            `,
        },
        "E0132",
    )
}

test "ordering_on_union_operands_is_rejected" {
    root.compiler.AssertHasMessage(
        {
            "ordering_on_union_operands_is_rejected.baml": `
                function f(a: int | string, b: int | string) -> bool {
                    a < b
                }
            `,
        },
        "does not implement `Compare`",
    )
}

test "ordering_on_interface_existential_is_rejected" {
    root.compiler.AssertHasMessage(
        {
            "ordering_on_interface_existential_is_rejected.baml": `
                function f(a: baml.ops.Compare, b: baml.ops.Compare) -> bool {
                    a < b
                }
            `,
        },
        "does not implement `Compare`",
    )
}

test "compare_bound_rejects_abstract_type_argument" {
    root.compiler.AssertHasCode(
        {
            "compare_bound_rejects_abstract_type_argument.baml": `
                function max<T extends baml.ops.Compare>(a: T, b: T) -> T throws never {
                    if a < b { b } else { a }
                }
                function f(a: int | string, b: int | string) -> int | string throws never {
                    max<int | string>(a, b)
                }
            `,
        },
        "E0001",
    )
}

test "ordering_on_interface_existential_type_argument_is_rejected" {
    root.compiler.AssertHasCode(
        {
            "ordering_on_interface_existential_type_argument_is_rejected.baml": `
                function max<T extends baml.ops.Compare>(a: T, b: T) -> T throws never {
                    if a < b { b } else { a }
                }
                function f(a: baml.ops.Compare, b: baml.ops.Compare) -> baml.ops.Compare throws never {
                    max<baml.ops.Compare>(a, b)
                }
            `,
        },
        "E0001",
    )
}

test "compare_without_equals_is_rejected" {
    root.compiler.AssertHasCode(
        {
            "compare_without_equals_is_rejected.baml": `
                class NoEq {
                    v: int
                    implements baml.ops.Compare {
                        function cmp(self, other: Self) -> baml.ops.Ordering throws never {
                            self.v.cmp(other.v)
                        }
                    }
                }
            `,
        },
        "E0125",
    )
}

test "arithmetic_on_user_type_without_impl_is_rejected" {
    root.compiler.AssertHasMessage(
        {
            "arithmetic_on_user_type_without_impl_is_rejected.baml": `
                class Vec2 { x: int }
                function f(a: Vec2, b: Vec2) -> Vec2 {
                    a + b
                }
            `,
        },
        "cannot be applied",
    )
}

test "negate_on_user_type_without_impl_is_rejected" {
    root.compiler.AssertHasMessage(
        {
            "negate_on_user_type_without_impl_is_rejected.baml": `
                class Vec2 { x: int }
                function f(a: Vec2) -> Vec2 {
                    -a
                }
            `,
        },
        "cannot be applied",
    )
}

test "arithmetic_on_existential_without_pinned_output_is_rejected" {
    root.compiler.AssertHasMessage(
        {
            "arithmetic_on_existential_without_pinned_output_is_rejected.baml": `
                function f(x: baml.ops.Add<int>, a: int) -> int {
                    x + a
                }
            `,
        },
        "cannot be applied",
    )
}

test "negate_on_bounded_typevar_without_pinned_output_is_rejected" {
    root.compiler.AssertHasMessage(
        {
            "negate_on_bounded_typevar_without_pinned_output_is_rejected.baml": `
                function f<T extends baml.ops.Negate>(x: T) -> int {
                    -x;
                    0
                }
            `,
        },
        "cannot be applied",
    )
}

test "arithmetic_on_bounded_typevar_without_pinned_output_is_rejected" {
    root.compiler.AssertHasMessage(
        {
            "arithmetic_on_bounded_typevar_without_pinned_output_is_rejected.baml": `
                function f<T extends baml.ops.Add<int>>(x: T) -> int {
                    x + 1;
                    0
                }
            `,
        },
        "cannot be applied",
    )
}

test "compound_assign_result_not_assignable_to_target_is_rejected" {
    root.compiler.AssertHasMessage(
        {
            "compound_assign_result_not_assignable_to_target_is_rejected.baml": `
                class Counter {
                    n: int
                    implements baml.ops.Add<int> {
                        type Output = int
                        function add(self, rhs: int) -> int throws never { self.n + rhs }
                    }
                }
                function f(c: Counter) -> Counter {
                    c += 1;
                    c
                }
            `,
        },
        "mismatched types",
    )
}

// BEGIN CUSTOM DIAGNOSTIC PORTS

test "diagnostic_codes_in_expected_range" {
    root.compiler.AssertHasCode(
        {
            "missing_method.baml": `interface I { function f(self) -> string } class C { implements I {} }`,
        },
        "E0113",
    );
    root.compiler.AssertHasCode(
        { "missing_interface.baml": `class C { implements Missing {} }` },
        "E0002",
    );
    root.compiler.AssertHasCode(
        { "class_target.baml": `class X { x: int } class C { implements X {} }` },
        "E0119",
    )
}

test "interface_default_method_body_gets_exhaustiveness_checking" {
    root.compiler.AssertHasCode(
        {
            "interface_default_method_body_gets_exhaustiveness_checking.baml": `
                interface Labels {
                    function label(self, value: bool) -> string throws never {
                        return match (value) {
                            true => "yes"
                        }
                    }
                }

                class Thing {
                    implements Labels {}
                }
            `,
        },
        "E0062",
    )
}

test "match_without_catchall_on_interface_is_compile_error" {
    root.compiler.AssertHasCode(
        {
            "match_without_catchall_on_interface_is_compile_error.baml": `
                interface Animal {
                    function speak(self) -> string throws never
                }
                class Dog {
                    implements Animal {
                        function speak(self) -> string { return "Woof!" }
                    }
                }
                class Cat {
                    implements Animal {
                        function speak(self) -> string { return "Meow." }
                    }
                }

                function describe(a: Animal) -> string {
                    return match (a) {
                        let d: Dog => "dog"
                        let c: Cat => "cat"
                    }
                }
            `,
        },
        "E0062",
    )
}

test "out_of_body_implements_for_unknown_target_is_error" {
    root.compiler.AssertHasMessage(
        {
            "out_of_body_implements_for_unknown_target_is_error.baml": `
                interface ToJson {
                    function to_json(self) -> string throws never
                }
                implements ToJson for Nonexistent {
                    function to_json(self) -> string { return "?" }
                }
            `,
        },
        "Nonexistent",
    )
}

test "unresolved_for_target_reported_even_when_interface_unresolved" {
    root.compiler.AssertHasMessage(
        {
            "unresolved_for_target_reported_even_when_interface_unresolved.baml": `
                implement BadInterface for AlsoMissing {}
            `,
        },
        "AlsoMissing",
    )
}

test "stolen_turbofish_on_generic_class_impl_static_is_diagnosed_not_ice" {
    root.compiler.AssertHasMessage(
        {
            "stolen_turbofish_on_generic_class_impl_static_is_diagnosed_not_ice.baml": `
                interface Picker {
                    function pick<X>() -> int throws never
                }
                class PickBin<T> {
                    value: T
                    implements Picker {
                        function pick<X>() -> int { return 7 }
                    }
                }
                function main() -> int {
                    return PickBin.pick<int>()
                }
            `,
        },
        "cannot infer type parameter",
    )
}

test "unqualified_call_on_ambiguous_class_errors" {
    AssertHasDiagnostic(
        {
            "unqualified_call_on_ambiguous_class_errors.baml": `
                interface Serializer {
                    function encode(self) -> string throws never
                }
                interface BinarySerializer {
                    function encode(self) -> string throws never
                }
                class Hybrid {
                    implements Serializer {
                        function encode(self) -> string { return "json" }
                    }
                    implements BinarySerializer {
                        function encode(self) -> string { return "bin" }
                    }
                }
                function main() -> string {
                    let h = Hybrid {}
                    return h.encode()
                }
            `,
        },
        "E0121",
        ["`Serializer`", "`BinarySerializer`"],
    )
}

test "three_way_unqualified_call_lists_every_source" {
    AssertHasDiagnostic(
        {
            "three_way_unqualified_call_lists_every_source.baml": `
                interface A {
                    function id(self) -> string throws never
                }
                interface B {
                    function id(self) -> string throws never
                }
                interface C {
                    function id(self) -> string throws never
                }

                class Tri {
                    implements A {
                        function id(self) -> string { return "a" }
                    }
                    implements B {
                        function id(self) -> string { return "b" }
                    }
                    implements C {
                        function id(self) -> string { return "c" }
                    }
                }
                function main() -> string {
                    let t = Tri {}
                    return t.id()
                }
            `,
        },
        "E0121",
        ["`A`", "`B`", "`C`"],
    )
}

test "bounded_blanket_impl_does_not_discharge_against_unrelated_env_param" {
    AssertHasDiagnostic(
        {
            "bounded_blanket_impl_does_not_discharge_against_unrelated_env_param.baml": `
                interface ProbeMarker {
                    function probe_tag(self) -> int throws never
                }
                class ProbeBin<T> {
                    value: T[]
                }
                implements<T extends ProbeMarker> ProbeMarker for ProbeBin<T> {
                    function probe_tag(self) -> int { return 1 }
                }
                function probe_caller<T extends ProbeMarker>(x: T) -> int {
                    let items = [];
                    let b = ProbeBin { value: items };
                    let n = b.probe_tag();
                    items.push(3);
                    return n
                }
                function main() -> int {
                    return 0
                }
            `,
        },
        "E0001",
        ["expected `ProbeMarker`, found `int`"],
    )
}

test "union_fuzz_f04_string_plus_int_is_type_error_not_vm_crash" {
    AssertHasDiagnostic(
        {
            "union_fuzz_f04_string_plus_int_is_type_error_not_vm_crash.baml": `
                function plainPlus() -> string {
                  return "age=" + 4
                }
                function main() -> string {
                  return plainPlus()
                }
            `,
        },
        "E0004",
        ["operator `+`"],
    )
}

test "interface_destructure_before_concrete_destructure_is_unreachable_not_nonexhaustive" {
    root.compiler.AssertHasCode(
        {
            "interface_destructure_before_concrete_destructure_is_unreachable_not_nonexhaustive.baml": `
                interface Animal {
                    name: string
                }
                class Dog {
                    name: string
                    breed: string
                    implements Animal {}
                }

                function describe(a: Animal) -> string {
                    return match (a) {
                        Animal { name: string } => "animal"
                        Dog { breed: "Lab" } => "lab"
                    }
                }
            `,
        },
        "E0063",
    );
    AssertLacksCode(
        {
            "interface_destructure_before_concrete_destructure_is_unreachable_not_nonexhaustive.baml": `
                interface Animal {
                    name: string
                }
                class Dog {
                    name: string
                    breed: string
                    implements Animal {}
                }

                function describe(a: Animal) -> string {
                    return match (a) {
                        Animal { name: string } => "animal"
                        Dog { breed: "Lab" } => "lab"
                    }
                }
            `,
        },
        "E0062",
    )
}

test "required_parent_lookup_uses_declaring_interface_namespace" {
    root.compiler.AssertHasCode(
        {
            "main.baml": `
                interface Parent {}

                class Robot {
                    function label(self) -> string { return "robot" }
                    implements root.contracts.Child {}
                    implements Parent {}
                }
            `,
            "ns_contracts/interfaces.baml": `
                interface Parent {
                    function label(self) -> string throws never
                }
                interface Child requires Parent {}
            `,
        },
        "E0125",
    )
}

test "out_of_body_empty_implements_for_generic_class_does_not_satisfy_via_inherent_method" {
    root.compiler.AssertHasCode(
        {
            "main.baml": `
                interface Printable {
                    function label(self) -> string throws never
                }

                implements Printable for root.models.Box<int> {}
            `,
            "ns_models/box.baml": `
                class Box<T> {
                    value: T

                    function label(self) -> string {
                        return "box"
                    }
                }
            `,
        },
        "E0113",
    )
}

test "unified_rule_namespaced_classes_with_same_short_name_do_not_cross_match" {
    root.compiler.AssertHasMessage(
        {
            "main.baml": `
                function take(item: root.a.Printable) -> string {
                    return item.display()
                }
                function bad() -> string {
                    return take(root.b.Wrapper { value: 42 })
                }
            `,
            "ns_a/wrapper.baml": `
                interface Printable {
                    function display(self) -> string throws never
                }
                class Wrapper {
                    value: int
                }
                implements Printable for Wrapper {
                    function display(self) -> string { return "a" }
                }
            `,
            "ns_b/wrapper.baml": `
                class Wrapper {
                    value: int
                }
            `,
        },
        "b.Wrapper",
    )
}

test "unified_rule_namespaced_generic_classes_with_same_short_name_do_not_cross_match" {
    root.compiler.AssertHasMessage(
        {
            "main.baml": `
                function take(item: root.a.Printable<int>) -> string {
                    return item.display()
                }
                function bad(item: root.b.Wrapper<int>) -> string {
                    return take(item)
                }
            `,
            "ns_a/wrapper.baml": `
                interface Printable<T> {
                    function display(self) -> string throws never
                }
                class Wrapper<T> {
                    value: T
                }
                implements<T> Printable<T> for Wrapper<T> {
                    function display(self) -> string { return "a" }
                }
            `,
            "ns_b/wrapper.baml": `
                class Wrapper<T> {
                    value: T
                }
            `,
        },
        "b.Wrapper<int>",
    )
}

test "namespaced_class_cannot_use_unrooted_cross_namespace_qualification" {
    root.compiler.AssertHasMessage(
        {
            "ns_a/a.baml": `
                interface Named {
                    function name(self) -> string throws never
                }
            `,
            "ns_pets/pets.baml": `
                class Dog {
                    implements a.Named {
                        function name(self) -> string { return "Rex" }
                    }
                }
            `,
        },
        "unresolved type: a.Named",
    )
}

test "qualified_generic_constructor_preserves_concrete_type_in_diagnostics" {
    root.compiler.AssertHasMessage(
        {
            "main.baml": `
                interface Printable<T> {
                    function display(self) -> string throws never
                }
                class Box<T> {
                    value: T
                }
                function take(item: Printable<int>) -> string {
                    return item.display()
                }
                function bad() -> string {
                    return take(root.b.Box<int> { value: 42 })
                }
            `,
            "ns_b/box.baml": `
                class Box<T> {
                    value: T
                }
            `,
        },
        "b.Box<int>",
    )
}

test "wf3_ambiguous_method_suggestion_resolvable_inside_namespace" {
    root.compiler.AssertHasMessage(
        {
            "main.baml": `function main() -> string { let p = birds.Parrot {} return p.run() }`,
            "ns_birds/birds.baml": `
                class Parrot {
                    implements root.zoo.Animal { function speak(self) -> string { return "squawk" } }
                    implements root.farm.Animal { function speak(self) -> string { return "cluck" } }
                    function run(self) -> string { return self.speak() }
                }
            `,
            "ns_zoo/zoo.baml": `interface Animal { function speak(self) -> string throws never }`,
            "ns_farm/farm.baml": `interface Animal { function speak(self) -> string throws never }`,
        },
        "root.zoo.Animal",
    )
}

test "wf3_bare_cross_ns_requires_unknown_echoes_qualifier" {
    root.compiler.AssertHasMessage(
        {
            "main.baml": `function main() -> string { return "ok" }`,
            "ns_a/a.baml": `interface Base { function f(self) -> string throws never }`,
            "ns_b/b.baml": `interface Derived requires root.a.Ghost {}`,
        },
        "root.a.Ghost",
    )
}

test "selfless_interface_method_is_not_reachable_through_a_value" {
    AssertHasDiagnostic(
        {
            "selfless_interface_method_is_not_reachable_through_a_value_0.baml": `
                interface Buildable {
                    function build(seed: int) -> Self
                }
                class Gadget {
                    implements Buildable {
                        function build(seed: int) -> Self throws never { Gadget {} }
                    }
                }
                function main() -> void {
                    let g = Gadget {}
                    g.build(1)
                }
            `,
        },
        "E0001",
        ["on interface `Buildable`"],
    );
    AssertHasDiagnostic(
        {
            "selfless_interface_method_is_not_reachable_through_a_value_1.baml": `
                interface Buildable {
                    function build(seed: int) -> Self
                }
                class Gadget {
                    implements Buildable {
                        function build(seed: int) -> Self throws never { Gadget {} }
                    }
                }
                function main() -> void {
                    let g = Gadget {}
                    let f = g.build
                }
            `,
        },
        "E0001",
        ["on interface `Buildable`"],
    )
}

test "selfless_inherent_method_is_not_reachable_through_a_value" {
    AssertHasDiagnostic(
        {
            "selfless_inherent_method_is_not_reachable_through_a_value_0.baml": `
                class Widget {
                    n: int
                    function make(seed: int) -> Widget throws never { Widget { n: seed } }
                }
                function main() -> void {
                    let w = Widget { n: 0 }
                    w.make(1)
                }
            `,
        },
        "E0001",
        ["`TypeName.make(...)`"],
    );
    AssertHasDiagnostic(
        {
            "selfless_inherent_method_is_not_reachable_through_a_value_1.baml": `
                class Widget {
                    n: int
                    function make(seed: int) -> Widget throws never { Widget { n: seed } }
                }
                function main() -> void {
                    let w = Widget { n: 0 }
                    let f = w.make
                }
            `,
        },
        "E0001",
        ["`TypeName.make(...)`"],
    )
}

test "form1_bounded_generic_receiver_rejects_when_bound_not_satisfied" {
    root.compiler.AssertHasMessage(
        {
            "form1_bounded_generic_receiver_rejects_when_bound_not_satisfied.baml": `
                interface Named {
                    name: string
                }
                interface Printable {
                    function display(self) -> string throws never
                }
                class Rock {
                    label: string
                }
                class Wrapper<T> {
                    inner: T
                }
                implements<T extends Named> Printable for Wrapper<T> {
                    function display(self) -> string { return self.inner.name }
                }
                function bad() -> string {
                    let item: Printable = Wrapper<Rock> { inner: Rock { label: "igneous" } }
                    return item.display()
                }
            `,
        },
        "Wrapper",
    );
    root.compiler.AssertHasMessage(
        {
            "form1_bounded_generic_receiver_rejects_when_bound_not_satisfied.baml": `
                interface Named {
                    name: string
                }
                interface Printable {
                    function display(self) -> string throws never
                }
                class Rock {
                    label: string
                }
                class Wrapper<T> {
                    inner: T
                }
                implements<T extends Named> Printable for Wrapper<T> {
                    function display(self) -> string { return self.inner.name }
                }
                function bad() -> string {
                    let item: Printable = Wrapper<Rock> { inner: Rock { label: "igneous" } }
                    return item.display()
                }
            `,
        },
        "Rock",
    )
}

test "malformed_impls_do_not_spuriously_overlap" {
    AssertCodeCount(
        {
            "malformed_impls_do_not_spuriously_overlap.baml": `
                interface Marker {}
                implement Marker for Nonexistent1 {}
                implement Marker for Nonexistent2 {}
            `,
        },
        "E0002",
        2,
    );
    AssertLacksCode(
        {
            "malformed_impls_do_not_spuriously_overlap.baml": `
                interface Marker {}
                implement Marker for Nonexistent1 {}
                implement Marker for Nonexistent2 {}
            `,
        },
        "E0132",
    )
}

test "implements_target_arg_error_is_reported_exactly_once" {
    root.compiler.AssertHasCode(
        {
            "implements_target_arg_error_is_reported_exactly_once_in_body.baml": `
                interface Container<T> {
                    function size(self) -> int throws never
                }
                class Box {
                    items: int[]
                    implements Container<DoesNotExist> {
                        function size(self) -> int { return 0 }
                    }
                }
            `,
        },
        "E0002",
    );
    root.compiler.AssertMessageCount(
        {
            "implements_target_arg_error_is_reported_exactly_once_in_body.baml": `
                interface Container<T> {
                    function size(self) -> int throws never
                }
                class Box {
                    items: int[]
                    implements Container<DoesNotExist> {
                        function size(self) -> int { return 0 }
                    }
                }
            `,
        },
        "DoesNotExist",
        1,
    );
    root.compiler.AssertMessageCount(
        {
            "implements_target_arg_error_is_reported_exactly_once_out_of_body.baml": `
                interface Container<T> {
                    function size(self) -> int throws never
                }
                class Cat {
                    name: string
                }
                implement Container<DoesNotExist> for Cat {
                    function size(self) -> int { return 0 }
                }
            `,
        },
        "DoesNotExist",
        1,
    )
}

test "requires_unknown_name_is_unknown_interface_not_non_interface" {
    root.compiler.AssertHasCode(
        {
            "requires_unknown_name_is_unknown_interface_not_non_interface.baml": `
                interface Person requires DoesNotExist {
                    name: string
                }
            `,
        },
        "E0002",
    );
    AssertLacksCode(
        {
            "requires_unknown_name_is_unknown_interface_not_non_interface.baml": `
                interface Person requires DoesNotExist {
                    name: string
                }
            `,
        },
        "E0133",
    )
}

test "ambiguous_method_across_namespaces_uses_qualified_interface_names" {
    AssertHasDiagnostic(
        {
            "main.baml": `
                class Parrot {
                    implements zoo.Animal {
                        function speak(self) -> string { return "squawk" }
                    }
                    implements farm.Animal {
                        function speak(self) -> string { return "cluck" }
                    }
                }
                function main() -> string {
                    let p = Parrot {}
                    return p.speak()
                }
            `,
            "ns_zoo/zoo.baml": `interface Animal { function speak(self) -> string throws never }`,
            "ns_farm/farm.baml": `interface Animal { function speak(self) -> string throws never }`,
        },
        "E0121",
        ["zoo.Animal", "farm.Animal", "as<zoo.Animal>", "as<farm.Animal>"],
    )
}

test "wf3_self_return_wrong_concrete_class_is_rejected" {
    AssertHasErrors(
        {
            "wf3_self_return_wrong_concrete_class_is_rejected.baml": `
                interface Cloneable {
                    function clone(self) -> Self throws never
                }
                class Box {
                    boxField: int
                    implements Cloneable {
                        function clone(self) -> Self {
                            return Cup { cupField: 99 }
                        }
                    }
                }
                class Cup {
                    cupField: int
                    implements Cloneable {
                        function clone(self) -> Self {
                            return Cup { cupField: self.cupField }
                        }
                    }
                }
                function main() -> int {
                    let b = Box { boxField: 1 }
                    let c = b.clone()
                    return c.boxField
                }
            `,
        },
    )
}

test "wf3_monomorph_collision_assignment_is_diagnosed" {
    AssertHasErrors(
        {
            "wf3_monomorph_collision_assignment_is_diagnosed.baml": `
                interface Getter<T> {
                    function get(self) -> T throws never
                }
                class Pair<L, R> {
                    left: L
                    right: R
                    implements Getter<L> {
                        function get(self) -> L { return self.left }
                    }
                    implements Getter<R> {
                        function get(self) -> R { return self.right }
                    }
                }
                function main() -> int {
                    let p: Pair<int, int> = Pair { left: 7, right: 99 }
                    let g: Getter<int> = p
                    return g.get()
                }
            `,
        },
    )
}

test "wf3_self_in_field_type_has_no_contradictory_diagnostics" {
    let advice_is_self_opt = false;
    for (
        let diagnostic in ErrorDiagnostics(
            {
                "wf3_self_in_field_type_has_no_contradictory_diagnostics_concrete.baml": `
                    interface Node {
                        value: int
                        next: Self?
                    }
                    class LinkedItem {
                        value: int
                        next: LinkedItem?
                        implements Node {}
                    }
                    function main() -> int {
                        let tail = LinkedItem { value: 2, next: null }
                        let head = LinkedItem { value: 1, next: tail }
                        return match (head.next) {
                            let n: LinkedItem => n.value,
                            _ => -1,
                        }
                    }
                `,
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
                "wf3_self_in_field_type_has_no_contradictory_diagnostics_self.baml": `
                    interface Node {
                        value: int
                        next: Self?
                    }
                    class LinkedItem {
                        value: int
                        next: Self?
                        implements Node {}
                    }
                    function main() -> int { return 0 }
                `,
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
            "wf3_method_on_interface_union_blames_correct_member.baml": `
                interface Animal {
                    function speak(self) -> string throws never
                }
                interface Swimmer {
                    function swim(self) -> string throws never
                }
                class Dog {
                    implements Animal {
                        function speak(self) -> string { return "Woof!" }
                    }
                    implements Swimmer {
                        function swim(self) -> string { return "splash" }
                    }
                }
                function describe(x: Animal | Swimmer) -> string {
                    return x.speak()
                }
                function main() -> string {
                    let a: Animal = Dog {}
                    return describe(a)
                }
            `,
        },
        "no common interface that declares",
    );
    AssertLacksDiagnostic(
        {
            "wf3_method_on_interface_union_blames_correct_member.baml": `
                interface Animal {
                    function speak(self) -> string throws never
                }
                interface Swimmer {
                    function swim(self) -> string throws never
                }
                class Dog {
                    implements Animal {
                        function speak(self) -> string { return "Woof!" }
                    }
                    implements Swimmer {
                        function swim(self) -> string { return "splash" }
                    }
                }
                function describe(x: Animal | Swimmer) -> string {
                    return x.speak()
                }
                function main() -> string {
                    let a: Animal = Dog {}
                    return describe(a)
                }
            `,
        },
        ["`Animal` has no member"],
    )
}

test "phantom_impl_param_call_is_diagnosed_not_ice" {
    root.compiler.AssertHasCode(
        {
            "phantom_impl_param_call_is_diagnosed_not_ice.baml": `
                interface PhantomMarker {
                    function tag(self) -> int throws never
                }
                class PhantomBin<T> {
                    value: T
                }
                implements<T, U> PhantomMarker for PhantomBin<T> {
                    function tag(self) -> int { return 1 }
                }
                function use_marker(b: PhantomBin<int>) -> int {
                    return b.tag()
                }
                function main() -> int {
                    return use_marker(PhantomBin { value: 3 })
                }
            `,
        },
        "E0135",
    )
}

test "union_fuzz_pr_ambiguous_inherited_method_in_class_union_is_e0121" {
    root.compiler.AssertHasCode(
        {
            "union_fuzz_pr_ambiguous_inherited_method_in_class_union_is_e0121.baml": `
                interface A { function f(self) -> string  throws never { return "a" } }
                interface B { function f(self) -> string  throws never { return "b" } }
                class C { implements A {} implements B {} }
                class D { implements A {} implements B {} }
                function g(x: C | D) -> string { return x.f() }
                function main() -> string { return g(C {}) }
            `,
        },
        "E0121",
    );
    AssertAllErrorsLack(
        {
            "union_fuzz_pr_ambiguous_inherited_method_in_class_union_is_e0121.baml": `
                interface A { function f(self) -> string  throws never { return "a" } }
                interface B { function f(self) -> string  throws never { return "b" } }
                class C { implements A {} implements B {} }
                class D { implements A {} implements B {} }
                function g(x: C | D) -> string { return x.f() }
                function main() -> string { return g(C {}) }
            `,
        },
        ["unknown |", "is not a function"],
    )
}

test "union_fuzz_pr_concrete_arm_not_callable_despite_recovery_arm" {
    root.compiler.AssertHasCode(
        {
            "union_fuzz_pr_concrete_arm_not_callable_despite_recovery_arm.baml": `
                function f(x: int | DoesNotExist) -> string { return x() }
                function main() -> string { return "ok" }
            `,
        },
        "E0006",
    )
}

test "union_fuzz_f12_iface_union_call_diagnostic_does_not_leak_internals" {
    AssertAllErrorsLack(
        {
            "union_fuzz_f12_iface_union_call_diagnostic_does_not_leak_internals.baml": `
                interface Animal {
                  function speak(self) -> string throws never
                }
                class Dog {
                  implements Animal {
                    function speak(self) -> string { return "Woof" }
                  }
                }
                class Cat {
                  implements Animal {
                    function speak(self) -> string { return "Meow" }
                  }
                }
                function describe(x: Animal | Cat) -> string {
                  return x.speak()
                }
                function main() -> string {
                  let a: Animal = Dog {}
                  return describe(a)
                }
            `,
        },
        ["throws never", "(self:"],
    )
}

test "union_fuzz_f13_exhaustiveness_witness_does_not_leak_user_prefix" {
    root.compiler.AssertHasCode(
        {
            "union_fuzz_f13_exhaustiveness_witness_does_not_leak_user_prefix.baml": `
                class Person { name: string }
                class Dog { owner: Person }
                class Cat { lives: int }
                function describe(x: Dog | Cat) -> string {
                  match (x) {
                    Dog { owner: Person { name: "Alice" } } => "alice's dog"
                  }
                }
                function main() -> string {
                  let p: Person = Person { name: "Bob" }
                  let d: Dog = Dog { owner: p }
                  return describe(d)
                }
            `,
        },
        "E0062",
    );
    AssertAllErrorsLack(
        {
            "union_fuzz_f13_exhaustiveness_witness_does_not_leak_user_prefix.baml": `
                class Person { name: string }
                class Dog { owner: Person }
                class Cat { lives: int }
                function describe(x: Dog | Cat) -> string {
                  match (x) {
                    Dog { owner: Person { name: "Alice" } } => "alice's dog"
                  }
                }
                function main() -> string {
                  let p: Person = Person { name: "Bob" }
                  let d: Dog = Dog { owner: p }
                  return describe(d)
                }
            `,
        },
        ["user."],
    )
}

test "union_fuzz_f14_exhaustiveness_witness_names_interface_members" {
    let found = false;
    for (
        let diagnostic in ErrorDiagnostics(
            {
                "union_fuzz_f14_exhaustiveness_witness_names_interface_members.baml": `
                    interface Animal { function speak(self) -> string throws never }
                    interface Vehicle { function drive(self) -> string throws never }
                    class Dog { name: string  implements Animal { function speak(self) -> string { return "Woof" } } }
                    class Car { model: string  implements Vehicle { function drive(self) -> string { return "Vroom" } } }
                    function describe(x: Animal | Vehicle) -> string {
                      match (x) {
                        Dog { name } => "dog " + name
                        Car { model } => "car " + model
                      }
                    }
                    function main() -> string {
                      let c: Car = Car { model: "T" }
                      return describe(c)
                    }
                `,
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
            "union_fuzz_f15_union_method_blame_skips_satisfying_member.baml": `
                interface Debuggable { function debug(self) -> string throws never }
                implements Debuggable for int { function debug(self) -> string { return "int" } }
                class Dog { name: string }
                function main() -> string {
                  let x: int | Dog = 7
                  return x.debug()
                }
            `,
        },
        "",
        ["Dog", "debug"],
    );
    AssertLacksDiagnostic(
        {
            "union_fuzz_f15_union_method_blame_skips_satisfying_member.baml": `
                interface Debuggable { function debug(self) -> string throws never }
                implements Debuggable for int { function debug(self) -> string { return "int" } }
                class Dog { name: string }
                function main() -> string {
                  let x: int | Dog = 7
                  return x.debug()
                }
            `,
        },
        ["`int`", "debug"],
    )
}

test "scalar_arithmetic_matches_builtin_impl_matrix" {
    root.compiler.AssertCompiles(
        {
            "scalar_arithmetic_matches_builtin_impl_matrix_accepted.baml": `
                function add_int_int(lhs: int, rhs: int) -> int { lhs + rhs; 0 }
                function sub_int_int(lhs: int, rhs: int) -> int { lhs - rhs; 0 }
                function mul_int_int(lhs: int, rhs: int) -> int { lhs * rhs; 0 }
                function div_int_int(lhs: int, rhs: int) -> int { lhs / rhs; 0 }
                function rem_int_int(lhs: int, rhs: int) -> int { lhs % rhs; 0 }
                function add_int_float(lhs: int, rhs: float) -> int { lhs + rhs; 0 }
                function sub_int_float(lhs: int, rhs: float) -> int { lhs - rhs; 0 }
                function mul_int_float(lhs: int, rhs: float) -> int { lhs * rhs; 0 }
                function div_int_float(lhs: int, rhs: float) -> int { lhs / rhs; 0 }
                function rem_int_float(lhs: int, rhs: float) -> int { lhs % rhs; 0 }
                function add_int_bigint(lhs: int, rhs: bigint) -> int { lhs + rhs; 0 }
                function sub_int_bigint(lhs: int, rhs: bigint) -> int { lhs - rhs; 0 }
                function mul_int_bigint(lhs: int, rhs: bigint) -> int { lhs * rhs; 0 }
                function div_int_bigint(lhs: int, rhs: bigint) -> int { lhs / rhs; 0 }
                function rem_int_bigint(lhs: int, rhs: bigint) -> int { lhs % rhs; 0 }
                function add_float_int(lhs: float, rhs: int) -> int { lhs + rhs; 0 }
                function sub_float_int(lhs: float, rhs: int) -> int { lhs - rhs; 0 }
                function mul_float_int(lhs: float, rhs: int) -> int { lhs * rhs; 0 }
                function div_float_int(lhs: float, rhs: int) -> int { lhs / rhs; 0 }
                function rem_float_int(lhs: float, rhs: int) -> int { lhs % rhs; 0 }
                function add_float_float(lhs: float, rhs: float) -> int { lhs + rhs; 0 }
                function sub_float_float(lhs: float, rhs: float) -> int { lhs - rhs; 0 }
                function mul_float_float(lhs: float, rhs: float) -> int { lhs * rhs; 0 }
                function div_float_float(lhs: float, rhs: float) -> int { lhs / rhs; 0 }
                function rem_float_float(lhs: float, rhs: float) -> int { lhs % rhs; 0 }
                function add_bigint_int(lhs: bigint, rhs: int) -> int { lhs + rhs; 0 }
                function sub_bigint_int(lhs: bigint, rhs: int) -> int { lhs - rhs; 0 }
                function mul_bigint_int(lhs: bigint, rhs: int) -> int { lhs * rhs; 0 }
                function div_bigint_int(lhs: bigint, rhs: int) -> int { lhs / rhs; 0 }
                function rem_bigint_int(lhs: bigint, rhs: int) -> int { lhs % rhs; 0 }
                function add_bigint_bigint(lhs: bigint, rhs: bigint) -> int { lhs + rhs; 0 }
                function sub_bigint_bigint(lhs: bigint, rhs: bigint) -> int { lhs - rhs; 0 }
                function mul_bigint_bigint(lhs: bigint, rhs: bigint) -> int { lhs * rhs; 0 }
                function div_bigint_bigint(lhs: bigint, rhs: bigint) -> int { lhs / rhs; 0 }
                function rem_bigint_bigint(lhs: bigint, rhs: bigint) -> int { lhs % rhs; 0 }
                function add_string_string(lhs: string, rhs: string) -> int { lhs + rhs; 0 }
            `,
        },
    );
    AssertErrorCount(
        {
            "scalar_arithmetic_matches_builtin_impl_matrix_rejected.baml": `
                function add_int_string(lhs: int, rhs: string) -> int { lhs + rhs; 0 }
                function sub_int_string(lhs: int, rhs: string) -> int { lhs - rhs; 0 }
                function mul_int_string(lhs: int, rhs: string) -> int { lhs * rhs; 0 }
                function div_int_string(lhs: int, rhs: string) -> int { lhs / rhs; 0 }
                function rem_int_string(lhs: int, rhs: string) -> int { lhs % rhs; 0 }
                function add_int_bool(lhs: int, rhs: bool) -> int { lhs + rhs; 0 }
                function sub_int_bool(lhs: int, rhs: bool) -> int { lhs - rhs; 0 }
                function mul_int_bool(lhs: int, rhs: bool) -> int { lhs * rhs; 0 }
                function div_int_bool(lhs: int, rhs: bool) -> int { lhs / rhs; 0 }
                function rem_int_bool(lhs: int, rhs: bool) -> int { lhs % rhs; 0 }
                function add_int_null(lhs: int, rhs: null) -> int { lhs + rhs; 0 }
                function sub_int_null(lhs: int, rhs: null) -> int { lhs - rhs; 0 }
                function mul_int_null(lhs: int, rhs: null) -> int { lhs * rhs; 0 }
                function div_int_null(lhs: int, rhs: null) -> int { lhs / rhs; 0 }
                function rem_int_null(lhs: int, rhs: null) -> int { lhs % rhs; 0 }
                function add_float_bigint(lhs: float, rhs: bigint) -> int { lhs + rhs; 0 }
                function sub_float_bigint(lhs: float, rhs: bigint) -> int { lhs - rhs; 0 }
                function mul_float_bigint(lhs: float, rhs: bigint) -> int { lhs * rhs; 0 }
                function div_float_bigint(lhs: float, rhs: bigint) -> int { lhs / rhs; 0 }
                function rem_float_bigint(lhs: float, rhs: bigint) -> int { lhs % rhs; 0 }
                function add_float_string(lhs: float, rhs: string) -> int { lhs + rhs; 0 }
                function sub_float_string(lhs: float, rhs: string) -> int { lhs - rhs; 0 }
                function mul_float_string(lhs: float, rhs: string) -> int { lhs * rhs; 0 }
                function div_float_string(lhs: float, rhs: string) -> int { lhs / rhs; 0 }
                function rem_float_string(lhs: float, rhs: string) -> int { lhs % rhs; 0 }
                function add_float_bool(lhs: float, rhs: bool) -> int { lhs + rhs; 0 }
                function sub_float_bool(lhs: float, rhs: bool) -> int { lhs - rhs; 0 }
                function mul_float_bool(lhs: float, rhs: bool) -> int { lhs * rhs; 0 }
                function div_float_bool(lhs: float, rhs: bool) -> int { lhs / rhs; 0 }
                function rem_float_bool(lhs: float, rhs: bool) -> int { lhs % rhs; 0 }
                function add_float_null(lhs: float, rhs: null) -> int { lhs + rhs; 0 }
                function sub_float_null(lhs: float, rhs: null) -> int { lhs - rhs; 0 }
                function mul_float_null(lhs: float, rhs: null) -> int { lhs * rhs; 0 }
                function div_float_null(lhs: float, rhs: null) -> int { lhs / rhs; 0 }
                function rem_float_null(lhs: float, rhs: null) -> int { lhs % rhs; 0 }
                function add_bigint_float(lhs: bigint, rhs: float) -> int { lhs + rhs; 0 }
                function sub_bigint_float(lhs: bigint, rhs: float) -> int { lhs - rhs; 0 }
                function mul_bigint_float(lhs: bigint, rhs: float) -> int { lhs * rhs; 0 }
                function div_bigint_float(lhs: bigint, rhs: float) -> int { lhs / rhs; 0 }
                function rem_bigint_float(lhs: bigint, rhs: float) -> int { lhs % rhs; 0 }
                function add_bigint_string(lhs: bigint, rhs: string) -> int { lhs + rhs; 0 }
                function sub_bigint_string(lhs: bigint, rhs: string) -> int { lhs - rhs; 0 }
                function mul_bigint_string(lhs: bigint, rhs: string) -> int { lhs * rhs; 0 }
                function div_bigint_string(lhs: bigint, rhs: string) -> int { lhs / rhs; 0 }
                function rem_bigint_string(lhs: bigint, rhs: string) -> int { lhs % rhs; 0 }
                function add_bigint_bool(lhs: bigint, rhs: bool) -> int { lhs + rhs; 0 }
                function sub_bigint_bool(lhs: bigint, rhs: bool) -> int { lhs - rhs; 0 }
                function mul_bigint_bool(lhs: bigint, rhs: bool) -> int { lhs * rhs; 0 }
                function div_bigint_bool(lhs: bigint, rhs: bool) -> int { lhs / rhs; 0 }
                function rem_bigint_bool(lhs: bigint, rhs: bool) -> int { lhs % rhs; 0 }
                function add_bigint_null(lhs: bigint, rhs: null) -> int { lhs + rhs; 0 }
                function sub_bigint_null(lhs: bigint, rhs: null) -> int { lhs - rhs; 0 }
                function mul_bigint_null(lhs: bigint, rhs: null) -> int { lhs * rhs; 0 }
                function div_bigint_null(lhs: bigint, rhs: null) -> int { lhs / rhs; 0 }
                function rem_bigint_null(lhs: bigint, rhs: null) -> int { lhs % rhs; 0 }
                function add_string_int(lhs: string, rhs: int) -> int { lhs + rhs; 0 }
                function sub_string_int(lhs: string, rhs: int) -> int { lhs - rhs; 0 }
                function mul_string_int(lhs: string, rhs: int) -> int { lhs * rhs; 0 }
                function div_string_int(lhs: string, rhs: int) -> int { lhs / rhs; 0 }
                function rem_string_int(lhs: string, rhs: int) -> int { lhs % rhs; 0 }
                function add_string_float(lhs: string, rhs: float) -> int { lhs + rhs; 0 }
                function sub_string_float(lhs: string, rhs: float) -> int { lhs - rhs; 0 }
                function mul_string_float(lhs: string, rhs: float) -> int { lhs * rhs; 0 }
                function div_string_float(lhs: string, rhs: float) -> int { lhs / rhs; 0 }
                function rem_string_float(lhs: string, rhs: float) -> int { lhs % rhs; 0 }
                function add_string_bigint(lhs: string, rhs: bigint) -> int { lhs + rhs; 0 }
                function sub_string_bigint(lhs: string, rhs: bigint) -> int { lhs - rhs; 0 }
                function mul_string_bigint(lhs: string, rhs: bigint) -> int { lhs * rhs; 0 }
                function div_string_bigint(lhs: string, rhs: bigint) -> int { lhs / rhs; 0 }
                function rem_string_bigint(lhs: string, rhs: bigint) -> int { lhs % rhs; 0 }
                function sub_string_string(lhs: string, rhs: string) -> int { lhs - rhs; 0 }
                function mul_string_string(lhs: string, rhs: string) -> int { lhs * rhs; 0 }
                function div_string_string(lhs: string, rhs: string) -> int { lhs / rhs; 0 }
                function rem_string_string(lhs: string, rhs: string) -> int { lhs % rhs; 0 }
                function add_string_bool(lhs: string, rhs: bool) -> int { lhs + rhs; 0 }
                function sub_string_bool(lhs: string, rhs: bool) -> int { lhs - rhs; 0 }
                function mul_string_bool(lhs: string, rhs: bool) -> int { lhs * rhs; 0 }
                function div_string_bool(lhs: string, rhs: bool) -> int { lhs / rhs; 0 }
                function rem_string_bool(lhs: string, rhs: bool) -> int { lhs % rhs; 0 }
                function add_string_null(lhs: string, rhs: null) -> int { lhs + rhs; 0 }
                function sub_string_null(lhs: string, rhs: null) -> int { lhs - rhs; 0 }
                function mul_string_null(lhs: string, rhs: null) -> int { lhs * rhs; 0 }
                function div_string_null(lhs: string, rhs: null) -> int { lhs / rhs; 0 }
                function rem_string_null(lhs: string, rhs: null) -> int { lhs % rhs; 0 }
                function add_bool_int(lhs: bool, rhs: int) -> int { lhs + rhs; 0 }
                function sub_bool_int(lhs: bool, rhs: int) -> int { lhs - rhs; 0 }
                function mul_bool_int(lhs: bool, rhs: int) -> int { lhs * rhs; 0 }
                function div_bool_int(lhs: bool, rhs: int) -> int { lhs / rhs; 0 }
                function rem_bool_int(lhs: bool, rhs: int) -> int { lhs % rhs; 0 }
                function add_bool_float(lhs: bool, rhs: float) -> int { lhs + rhs; 0 }
                function sub_bool_float(lhs: bool, rhs: float) -> int { lhs - rhs; 0 }
                function mul_bool_float(lhs: bool, rhs: float) -> int { lhs * rhs; 0 }
                function div_bool_float(lhs: bool, rhs: float) -> int { lhs / rhs; 0 }
                function rem_bool_float(lhs: bool, rhs: float) -> int { lhs % rhs; 0 }
                function add_bool_bigint(lhs: bool, rhs: bigint) -> int { lhs + rhs; 0 }
                function sub_bool_bigint(lhs: bool, rhs: bigint) -> int { lhs - rhs; 0 }
                function mul_bool_bigint(lhs: bool, rhs: bigint) -> int { lhs * rhs; 0 }
                function div_bool_bigint(lhs: bool, rhs: bigint) -> int { lhs / rhs; 0 }
                function rem_bool_bigint(lhs: bool, rhs: bigint) -> int { lhs % rhs; 0 }
                function add_bool_string(lhs: bool, rhs: string) -> int { lhs + rhs; 0 }
                function sub_bool_string(lhs: bool, rhs: string) -> int { lhs - rhs; 0 }
                function mul_bool_string(lhs: bool, rhs: string) -> int { lhs * rhs; 0 }
                function div_bool_string(lhs: bool, rhs: string) -> int { lhs / rhs; 0 }
                function rem_bool_string(lhs: bool, rhs: string) -> int { lhs % rhs; 0 }
                function add_bool_bool(lhs: bool, rhs: bool) -> int { lhs + rhs; 0 }
                function sub_bool_bool(lhs: bool, rhs: bool) -> int { lhs - rhs; 0 }
                function mul_bool_bool(lhs: bool, rhs: bool) -> int { lhs * rhs; 0 }
                function div_bool_bool(lhs: bool, rhs: bool) -> int { lhs / rhs; 0 }
                function rem_bool_bool(lhs: bool, rhs: bool) -> int { lhs % rhs; 0 }
                function add_bool_null(lhs: bool, rhs: null) -> int { lhs + rhs; 0 }
                function sub_bool_null(lhs: bool, rhs: null) -> int { lhs - rhs; 0 }
                function mul_bool_null(lhs: bool, rhs: null) -> int { lhs * rhs; 0 }
                function div_bool_null(lhs: bool, rhs: null) -> int { lhs / rhs; 0 }
                function rem_bool_null(lhs: bool, rhs: null) -> int { lhs % rhs; 0 }
                function add_null_int(lhs: null, rhs: int) -> int { lhs + rhs; 0 }
                function sub_null_int(lhs: null, rhs: int) -> int { lhs - rhs; 0 }
                function mul_null_int(lhs: null, rhs: int) -> int { lhs * rhs; 0 }
                function div_null_int(lhs: null, rhs: int) -> int { lhs / rhs; 0 }
                function rem_null_int(lhs: null, rhs: int) -> int { lhs % rhs; 0 }
                function add_null_float(lhs: null, rhs: float) -> int { lhs + rhs; 0 }
                function sub_null_float(lhs: null, rhs: float) -> int { lhs - rhs; 0 }
                function mul_null_float(lhs: null, rhs: float) -> int { lhs * rhs; 0 }
                function div_null_float(lhs: null, rhs: float) -> int { lhs / rhs; 0 }
                function rem_null_float(lhs: null, rhs: float) -> int { lhs % rhs; 0 }
                function add_null_bigint(lhs: null, rhs: bigint) -> int { lhs + rhs; 0 }
                function sub_null_bigint(lhs: null, rhs: bigint) -> int { lhs - rhs; 0 }
                function mul_null_bigint(lhs: null, rhs: bigint) -> int { lhs * rhs; 0 }
                function div_null_bigint(lhs: null, rhs: bigint) -> int { lhs / rhs; 0 }
                function rem_null_bigint(lhs: null, rhs: bigint) -> int { lhs % rhs; 0 }
                function add_null_string(lhs: null, rhs: string) -> int { lhs + rhs; 0 }
                function sub_null_string(lhs: null, rhs: string) -> int { lhs - rhs; 0 }
                function mul_null_string(lhs: null, rhs: string) -> int { lhs * rhs; 0 }
                function div_null_string(lhs: null, rhs: string) -> int { lhs / rhs; 0 }
                function rem_null_string(lhs: null, rhs: string) -> int { lhs % rhs; 0 }
                function add_null_bool(lhs: null, rhs: bool) -> int { lhs + rhs; 0 }
                function sub_null_bool(lhs: null, rhs: bool) -> int { lhs - rhs; 0 }
                function mul_null_bool(lhs: null, rhs: bool) -> int { lhs * rhs; 0 }
                function div_null_bool(lhs: null, rhs: bool) -> int { lhs / rhs; 0 }
                function rem_null_bool(lhs: null, rhs: bool) -> int { lhs % rhs; 0 }
                function add_null_null(lhs: null, rhs: null) -> int { lhs + rhs; 0 }
                function sub_null_null(lhs: null, rhs: null) -> int { lhs - rhs; 0 }
                function mul_null_null(lhs: null, rhs: null) -> int { lhs * rhs; 0 }
                function div_null_null(lhs: null, rhs: null) -> int { lhs / rhs; 0 }
                function rem_null_null(lhs: null, rhs: null) -> int { lhs % rhs; 0 }
            `,
        },
        144,
    );
    AssertAllErrorsContain(
        {
            "scalar_arithmetic_matches_builtin_impl_matrix_rejected.baml": `
                function add_int_string(lhs: int, rhs: string) -> int { lhs + rhs; 0 }
                function sub_int_string(lhs: int, rhs: string) -> int { lhs - rhs; 0 }
                function mul_int_string(lhs: int, rhs: string) -> int { lhs * rhs; 0 }
                function div_int_string(lhs: int, rhs: string) -> int { lhs / rhs; 0 }
                function rem_int_string(lhs: int, rhs: string) -> int { lhs % rhs; 0 }
                function add_int_bool(lhs: int, rhs: bool) -> int { lhs + rhs; 0 }
                function sub_int_bool(lhs: int, rhs: bool) -> int { lhs - rhs; 0 }
                function mul_int_bool(lhs: int, rhs: bool) -> int { lhs * rhs; 0 }
                function div_int_bool(lhs: int, rhs: bool) -> int { lhs / rhs; 0 }
                function rem_int_bool(lhs: int, rhs: bool) -> int { lhs % rhs; 0 }
                function add_int_null(lhs: int, rhs: null) -> int { lhs + rhs; 0 }
                function sub_int_null(lhs: int, rhs: null) -> int { lhs - rhs; 0 }
                function mul_int_null(lhs: int, rhs: null) -> int { lhs * rhs; 0 }
                function div_int_null(lhs: int, rhs: null) -> int { lhs / rhs; 0 }
                function rem_int_null(lhs: int, rhs: null) -> int { lhs % rhs; 0 }
                function add_float_bigint(lhs: float, rhs: bigint) -> int { lhs + rhs; 0 }
                function sub_float_bigint(lhs: float, rhs: bigint) -> int { lhs - rhs; 0 }
                function mul_float_bigint(lhs: float, rhs: bigint) -> int { lhs * rhs; 0 }
                function div_float_bigint(lhs: float, rhs: bigint) -> int { lhs / rhs; 0 }
                function rem_float_bigint(lhs: float, rhs: bigint) -> int { lhs % rhs; 0 }
                function add_float_string(lhs: float, rhs: string) -> int { lhs + rhs; 0 }
                function sub_float_string(lhs: float, rhs: string) -> int { lhs - rhs; 0 }
                function mul_float_string(lhs: float, rhs: string) -> int { lhs * rhs; 0 }
                function div_float_string(lhs: float, rhs: string) -> int { lhs / rhs; 0 }
                function rem_float_string(lhs: float, rhs: string) -> int { lhs % rhs; 0 }
                function add_float_bool(lhs: float, rhs: bool) -> int { lhs + rhs; 0 }
                function sub_float_bool(lhs: float, rhs: bool) -> int { lhs - rhs; 0 }
                function mul_float_bool(lhs: float, rhs: bool) -> int { lhs * rhs; 0 }
                function div_float_bool(lhs: float, rhs: bool) -> int { lhs / rhs; 0 }
                function rem_float_bool(lhs: float, rhs: bool) -> int { lhs % rhs; 0 }
                function add_float_null(lhs: float, rhs: null) -> int { lhs + rhs; 0 }
                function sub_float_null(lhs: float, rhs: null) -> int { lhs - rhs; 0 }
                function mul_float_null(lhs: float, rhs: null) -> int { lhs * rhs; 0 }
                function div_float_null(lhs: float, rhs: null) -> int { lhs / rhs; 0 }
                function rem_float_null(lhs: float, rhs: null) -> int { lhs % rhs; 0 }
                function add_bigint_float(lhs: bigint, rhs: float) -> int { lhs + rhs; 0 }
                function sub_bigint_float(lhs: bigint, rhs: float) -> int { lhs - rhs; 0 }
                function mul_bigint_float(lhs: bigint, rhs: float) -> int { lhs * rhs; 0 }
                function div_bigint_float(lhs: bigint, rhs: float) -> int { lhs / rhs; 0 }
                function rem_bigint_float(lhs: bigint, rhs: float) -> int { lhs % rhs; 0 }
                function add_bigint_string(lhs: bigint, rhs: string) -> int { lhs + rhs; 0 }
                function sub_bigint_string(lhs: bigint, rhs: string) -> int { lhs - rhs; 0 }
                function mul_bigint_string(lhs: bigint, rhs: string) -> int { lhs * rhs; 0 }
                function div_bigint_string(lhs: bigint, rhs: string) -> int { lhs / rhs; 0 }
                function rem_bigint_string(lhs: bigint, rhs: string) -> int { lhs % rhs; 0 }
                function add_bigint_bool(lhs: bigint, rhs: bool) -> int { lhs + rhs; 0 }
                function sub_bigint_bool(lhs: bigint, rhs: bool) -> int { lhs - rhs; 0 }
                function mul_bigint_bool(lhs: bigint, rhs: bool) -> int { lhs * rhs; 0 }
                function div_bigint_bool(lhs: bigint, rhs: bool) -> int { lhs / rhs; 0 }
                function rem_bigint_bool(lhs: bigint, rhs: bool) -> int { lhs % rhs; 0 }
                function add_bigint_null(lhs: bigint, rhs: null) -> int { lhs + rhs; 0 }
                function sub_bigint_null(lhs: bigint, rhs: null) -> int { lhs - rhs; 0 }
                function mul_bigint_null(lhs: bigint, rhs: null) -> int { lhs * rhs; 0 }
                function div_bigint_null(lhs: bigint, rhs: null) -> int { lhs / rhs; 0 }
                function rem_bigint_null(lhs: bigint, rhs: null) -> int { lhs % rhs; 0 }
                function add_string_int(lhs: string, rhs: int) -> int { lhs + rhs; 0 }
                function sub_string_int(lhs: string, rhs: int) -> int { lhs - rhs; 0 }
                function mul_string_int(lhs: string, rhs: int) -> int { lhs * rhs; 0 }
                function div_string_int(lhs: string, rhs: int) -> int { lhs / rhs; 0 }
                function rem_string_int(lhs: string, rhs: int) -> int { lhs % rhs; 0 }
                function add_string_float(lhs: string, rhs: float) -> int { lhs + rhs; 0 }
                function sub_string_float(lhs: string, rhs: float) -> int { lhs - rhs; 0 }
                function mul_string_float(lhs: string, rhs: float) -> int { lhs * rhs; 0 }
                function div_string_float(lhs: string, rhs: float) -> int { lhs / rhs; 0 }
                function rem_string_float(lhs: string, rhs: float) -> int { lhs % rhs; 0 }
                function add_string_bigint(lhs: string, rhs: bigint) -> int { lhs + rhs; 0 }
                function sub_string_bigint(lhs: string, rhs: bigint) -> int { lhs - rhs; 0 }
                function mul_string_bigint(lhs: string, rhs: bigint) -> int { lhs * rhs; 0 }
                function div_string_bigint(lhs: string, rhs: bigint) -> int { lhs / rhs; 0 }
                function rem_string_bigint(lhs: string, rhs: bigint) -> int { lhs % rhs; 0 }
                function sub_string_string(lhs: string, rhs: string) -> int { lhs - rhs; 0 }
                function mul_string_string(lhs: string, rhs: string) -> int { lhs * rhs; 0 }
                function div_string_string(lhs: string, rhs: string) -> int { lhs / rhs; 0 }
                function rem_string_string(lhs: string, rhs: string) -> int { lhs % rhs; 0 }
                function add_string_bool(lhs: string, rhs: bool) -> int { lhs + rhs; 0 }
                function sub_string_bool(lhs: string, rhs: bool) -> int { lhs - rhs; 0 }
                function mul_string_bool(lhs: string, rhs: bool) -> int { lhs * rhs; 0 }
                function div_string_bool(lhs: string, rhs: bool) -> int { lhs / rhs; 0 }
                function rem_string_bool(lhs: string, rhs: bool) -> int { lhs % rhs; 0 }
                function add_string_null(lhs: string, rhs: null) -> int { lhs + rhs; 0 }
                function sub_string_null(lhs: string, rhs: null) -> int { lhs - rhs; 0 }
                function mul_string_null(lhs: string, rhs: null) -> int { lhs * rhs; 0 }
                function div_string_null(lhs: string, rhs: null) -> int { lhs / rhs; 0 }
                function rem_string_null(lhs: string, rhs: null) -> int { lhs % rhs; 0 }
                function add_bool_int(lhs: bool, rhs: int) -> int { lhs + rhs; 0 }
                function sub_bool_int(lhs: bool, rhs: int) -> int { lhs - rhs; 0 }
                function mul_bool_int(lhs: bool, rhs: int) -> int { lhs * rhs; 0 }
                function div_bool_int(lhs: bool, rhs: int) -> int { lhs / rhs; 0 }
                function rem_bool_int(lhs: bool, rhs: int) -> int { lhs % rhs; 0 }
                function add_bool_float(lhs: bool, rhs: float) -> int { lhs + rhs; 0 }
                function sub_bool_float(lhs: bool, rhs: float) -> int { lhs - rhs; 0 }
                function mul_bool_float(lhs: bool, rhs: float) -> int { lhs * rhs; 0 }
                function div_bool_float(lhs: bool, rhs: float) -> int { lhs / rhs; 0 }
                function rem_bool_float(lhs: bool, rhs: float) -> int { lhs % rhs; 0 }
                function add_bool_bigint(lhs: bool, rhs: bigint) -> int { lhs + rhs; 0 }
                function sub_bool_bigint(lhs: bool, rhs: bigint) -> int { lhs - rhs; 0 }
                function mul_bool_bigint(lhs: bool, rhs: bigint) -> int { lhs * rhs; 0 }
                function div_bool_bigint(lhs: bool, rhs: bigint) -> int { lhs / rhs; 0 }
                function rem_bool_bigint(lhs: bool, rhs: bigint) -> int { lhs % rhs; 0 }
                function add_bool_string(lhs: bool, rhs: string) -> int { lhs + rhs; 0 }
                function sub_bool_string(lhs: bool, rhs: string) -> int { lhs - rhs; 0 }
                function mul_bool_string(lhs: bool, rhs: string) -> int { lhs * rhs; 0 }
                function div_bool_string(lhs: bool, rhs: string) -> int { lhs / rhs; 0 }
                function rem_bool_string(lhs: bool, rhs: string) -> int { lhs % rhs; 0 }
                function add_bool_bool(lhs: bool, rhs: bool) -> int { lhs + rhs; 0 }
                function sub_bool_bool(lhs: bool, rhs: bool) -> int { lhs - rhs; 0 }
                function mul_bool_bool(lhs: bool, rhs: bool) -> int { lhs * rhs; 0 }
                function div_bool_bool(lhs: bool, rhs: bool) -> int { lhs / rhs; 0 }
                function rem_bool_bool(lhs: bool, rhs: bool) -> int { lhs % rhs; 0 }
                function add_bool_null(lhs: bool, rhs: null) -> int { lhs + rhs; 0 }
                function sub_bool_null(lhs: bool, rhs: null) -> int { lhs - rhs; 0 }
                function mul_bool_null(lhs: bool, rhs: null) -> int { lhs * rhs; 0 }
                function div_bool_null(lhs: bool, rhs: null) -> int { lhs / rhs; 0 }
                function rem_bool_null(lhs: bool, rhs: null) -> int { lhs % rhs; 0 }
                function add_null_int(lhs: null, rhs: int) -> int { lhs + rhs; 0 }
                function sub_null_int(lhs: null, rhs: int) -> int { lhs - rhs; 0 }
                function mul_null_int(lhs: null, rhs: int) -> int { lhs * rhs; 0 }
                function div_null_int(lhs: null, rhs: int) -> int { lhs / rhs; 0 }
                function rem_null_int(lhs: null, rhs: int) -> int { lhs % rhs; 0 }
                function add_null_float(lhs: null, rhs: float) -> int { lhs + rhs; 0 }
                function sub_null_float(lhs: null, rhs: float) -> int { lhs - rhs; 0 }
                function mul_null_float(lhs: null, rhs: float) -> int { lhs * rhs; 0 }
                function div_null_float(lhs: null, rhs: float) -> int { lhs / rhs; 0 }
                function rem_null_float(lhs: null, rhs: float) -> int { lhs % rhs; 0 }
                function add_null_bigint(lhs: null, rhs: bigint) -> int { lhs + rhs; 0 }
                function sub_null_bigint(lhs: null, rhs: bigint) -> int { lhs - rhs; 0 }
                function mul_null_bigint(lhs: null, rhs: bigint) -> int { lhs * rhs; 0 }
                function div_null_bigint(lhs: null, rhs: bigint) -> int { lhs / rhs; 0 }
                function rem_null_bigint(lhs: null, rhs: bigint) -> int { lhs % rhs; 0 }
                function add_null_string(lhs: null, rhs: string) -> int { lhs + rhs; 0 }
                function sub_null_string(lhs: null, rhs: string) -> int { lhs - rhs; 0 }
                function mul_null_string(lhs: null, rhs: string) -> int { lhs * rhs; 0 }
                function div_null_string(lhs: null, rhs: string) -> int { lhs / rhs; 0 }
                function rem_null_string(lhs: null, rhs: string) -> int { lhs % rhs; 0 }
                function add_null_bool(lhs: null, rhs: bool) -> int { lhs + rhs; 0 }
                function sub_null_bool(lhs: null, rhs: bool) -> int { lhs - rhs; 0 }
                function mul_null_bool(lhs: null, rhs: bool) -> int { lhs * rhs; 0 }
                function div_null_bool(lhs: null, rhs: bool) -> int { lhs / rhs; 0 }
                function rem_null_bool(lhs: null, rhs: bool) -> int { lhs % rhs; 0 }
                function add_null_null(lhs: null, rhs: null) -> int { lhs + rhs; 0 }
                function sub_null_null(lhs: null, rhs: null) -> int { lhs - rhs; 0 }
                function mul_null_null(lhs: null, rhs: null) -> int { lhs * rhs; 0 }
                function div_null_null(lhs: null, rhs: null) -> int { lhs / rhs; 0 }
                function rem_null_null(lhs: null, rhs: null) -> int { lhs % rhs; 0 }
            `,
        },
        "cannot be applied",
    )
}

test "structural_arithmetic_without_impls_is_rejected" {
    AssertErrorCount(
        {
            "structural_arithmetic_without_impls_is_rejected.baml": `
                function f(xs: int[], values: map<string, int>, n: float) -> int {
                    xs + n;
                    n + xs;
                    values + n;
                    n + values;
                    0
                }
            `,
        },
        4,
    );
    AssertAllErrorsContain(
        {
            "structural_arithmetic_without_impls_is_rejected.baml": `
                function f(xs: int[], values: map<string, int>, n: float) -> int {
                    xs + n;
                    n + xs;
                    values + n;
                    n + values;
                    0
                }
            `,
        },
        "cannot be applied",
    )
}

test "conjunction_ambiguous_source_fields_diagnostic_names_ambiguity" {
    AssertHasDiagnostic(
        {
            "conjunction_ambiguous_source_fields_diagnostic_names_ambiguity.baml": `
                interface Left { value: string }
                interface Right { value: int }
                function read<T extends Left & Right>(value: T) -> string {
                    return value.value
                }
            `,
        },
        "",
        ["field `value` on class", "is ambiguous because it is declared by multiple interfaces"],
    )
}

test "ordering_on_different_concrete_types_is_rejected" {
    AssertHasDiagnostic(
        {
            "ordering_on_different_concrete_types_is_rejected.baml": `
                function f(a: int, b: string) -> bool {
                    a < b
                }
            `,
        },
        "",
        ["ordering requires both operands", "with `<`"],
    )
}

test "wf3_bare_generic_interface_reflection_is_arity_error" {
    root.compiler.AssertHasMessage(
        {
            "wf3_bare_generic_interface_reflection_is_arity_error.baml": `
                interface Box<T> {
                    function get(self) -> T throws never
                }
                class IntBox {
                    implements Box<int> {
                        function get(self) -> int { return 1 }
                    }
                }
                function main() -> bool {
                    let bare = reflect.Type.of<Box>()
                    return bare.implemented_by(reflect.Type.of<IntBox>())
                }
            `,
        },
        "type `Box` expects 1 type argument(s), got 0",
    )
}
