//! The reflection specialization surface on `reflect.function.Type`:
//! `is_generic`, `generic_params`, `specialize`, and the descriptor-side
//! `get<F>` (B-1582 item 3).
//!
//! `Package.functions()` lists every declared function, generics included. A
//! generic entry is a descriptor with no readable signature until `specialize`
//! supplies its type arguments; the specialized descriptor is anonymous, so its
//! own `get<F>` is the only door to the callable.

use baml_tests::baml_test;
use bex_engine::BexExternalValue;

/// A generic LLM function plus the concrete neighbours the truth table needs.
/// The client is never reached: only `$render_prompt` is invoked.
const GENERIC_SOURCE: &str = r#"
client TestClient = openai.ResponsesClient.new(
    model = "gpt-4o-mini",
    api_key = "test-key",
    base_url = "http://localhost:1234",
);

type PromptFn = (topic: string) -> ai.Prompt throws unknown;

function GenericList<T>(topic: string) -> T[] {
    client: TestClient
    prompt: `
        Return an empty list of ${topic}.
        ${ctx.output_format}
    `
}

function PlainList(topic: string) -> string[] {
    client: TestClient
    prompt: `
        Return an empty list of ${topic}.
        ${ctx.output_format}
    `
}
"#;

fn generic_source_with(main: &str) -> String {
    format!("{GENERIC_SOURCE}\n{main}")
}

async fn thrown_diagnostic(source: &str) -> String {
    let output = baml_test!(baml: source);
    match output.result {
        Ok(BexExternalValue::String(message)) => message.to_string(),
        other => panic!("expected a diagnostic string, got {other:?}"),
    }
}

/// `is_generic` is the guard the rest of the surface is documented against, so
/// pin all four cells: a generic function, its generated companion (companions
/// are specialized individually), a concrete function, and its companion.
#[tokio::test]
async fn is_generic_truth_table() {
    let output = baml_test!(baml: &generic_source_with(
        r#"
        function cell(flag: bool) -> string throws never {
            if (flag) { "1" } else { "0" }
        }

        function main() -> string throws unknown {
            let functions = reflect.Package.current().functions()
            let generic = functions.get("root.GenericList") ?? throw "GenericList not listed"
            let generic_companion = functions.get("root.GenericList$render_prompt")
                ?? throw "GenericList$render_prompt not listed"
            let concrete = functions.get("root.PlainList") ?? throw "PlainList not listed"
            let concrete_companion = functions.get("root.PlainList$render_prompt")
                ?? throw "PlainList$render_prompt not listed"
            cell(generic.is_generic())
                + cell(generic_companion.is_generic())
                + cell(concrete.is_generic())
                + cell(concrete_companion.is_generic())
        }
        "#,
    ));
    assert_eq!(output.result, Ok(BexExternalValue::String("1100".into())));
}

/// Names and count, in declaration order. Bounds are deliberately absent — see
/// the class docs on `reflect.function.GenericParam`.
#[tokio::test]
async fn generic_params_report_names_and_count() {
    let output = baml_test!(
        r#"
        function pair<K, V>(key: K, value: V) -> string { "" }
        function plain(value: string) -> string { value }

        function main() -> string throws unknown {
            let functions = reflect.Package.current().functions()
            let generic = functions.get("root.pair") ?? throw "pair not listed"
            let concrete = functions.get("root.plain") ?? throw "plain not listed"
            if (concrete.generic_params().length() != 0) {
                throw "a concrete function reported type parameters"
            } else { null }
            let names = generic.generic_params().map(
                (param: reflect.function.GenericParam) -> string { param.name },
            )
            names.join(",")
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::String("K,V".into())));
}

/// The whole flow with a statically spelled type argument: list, specialize,
/// extract through a function-type contract, call.
#[tokio::test]
async fn specialize_with_a_static_type_extracts_and_calls() {
    let output = baml_test!(
        r#"
        type IntFn = (value: int) -> int throws never;

        function identity<T>(value: T) -> T { value }

        function main() -> int throws unknown {
            let descriptor = reflect.Package.current().functions().get("root.identity")
                ?? throw "identity not listed"
            let specialized = descriptor.specialize([reflect.Type.of<int>()])
            if (specialized.is_generic()) { throw "still generic after specialize" } else { null }
            let callable = specialized.get<IntFn>()
                ?? throw "specialized descriptor yielded no callable"
            callable(7)
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(7)));
}

/// The specialized signature is the substituted one, readable through the
/// ordinary kind-view accessors that throw before specialization.
#[tokio::test]
async fn specialized_descriptor_reads_its_signature() {
    let output = baml_test!(
        r#"
        function identity<T>(value: T) -> T { value }

        function main() -> bool throws unknown {
            let descriptor = reflect.Package.current().functions().get("root.identity")
                ?? throw "identity not listed"
            let specialized = descriptor.specialize([reflect.Type.of<string>()])
            let param = specialized.params().at(0) ?? throw "no parameter"
            param.name == "value"
                && param.type == reflect.Type.of<string>()
                && specialized.return_type() == reflect.Type.of<string>()
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

/// B-1582 item 3, end to end with a **runtime-minted** type: compile a schema
/// package, list the generic LLM function's companion, specialize it with the
/// runtime class, extract it, and render. The prompt has to carry the runtime
/// class's own schema, which only works if the supplied value's definition
/// overlay reached the callee's frame.
#[tokio::test]
async fn specialize_with_a_runtime_type_renders_its_schema() {
    let output = baml_test!(baml: &generic_source_with(
        r#"
        function main() -> string throws unknown {
            let schema = reflect.Package.compile({
                "schema.baml": "class ExtractedRecord { account string amount int }",
            })
            let record = schema.get_class("root.ExtractedRecord") ?? throw "missing ExtractedRecord"

            let descriptor = reflect.Package.current().functions()
                .get("root.GenericList$render_prompt") ?? throw "companion not listed"
            let specialized = descriptor.specialize([record.as_type()])
            let render = specialized.get<PromptFn>() ?? throw "no callable"
            render("records").text()
        }
        "#,
    ));
    let Ok(BexExternalValue::String(prompt)) = output.result else {
        panic!("expected a rendered prompt, got {:?}", output.result)
    };
    assert!(prompt.contains("account"), "{prompt}");
    assert!(prompt.contains("amount"), "{prompt}");
}

/// Identity, both ways round: the supplied value is carried by reference, so
/// the specialized descriptor reports it back unchanged and the callable's own
/// frame materializes the same value rather than a re-minted equivalent.
#[tokio::test]
async fn specialize_preserves_the_supplied_mint() {
    let output = baml_test!(
        r#"
        function echo<T>(value: T) -> T { value }
        function reflected<T>() -> reflect.Type { reflect.Type.of<T>() }

        function main() -> bool throws unknown {
            let schema = reflect.Package.compile({
                "schema.baml": "class ExtractedRecord { account string amount int }",
            })
            let record = schema.get_class("root.ExtractedRecord") ?? throw "missing ExtractedRecord"
            let minted = record.as_type()
            let functions = reflect.Package.current().functions()

            // Descriptor side: the reported signature is the value itself, not
            // a structurally equal re-mint.
            let echoed = (functions.get("root.echo") ?? throw "echo not listed")
                .specialize([minted])
            let param = echoed.params().at(0) ?? throw "no parameter"

            // Callable side: the body's own `reflect.Type.of<T>()` resolves to it too.
            let reflected = (functions.get("root.reflected") ?? throw "reflected not listed")
                .specialize([minted])
            let callable = reflected.get<baml.AnyFunction<Returns = reflect.Type, Throws = unknown>>()
                ?? throw "no callable"
            let materialized = reflect.call_any<reflect.Type, unknown>(callable, {})

            param.type == minted && echoed.return_type() == minted && materialized == minted
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

/// Arity is checked against the parameters still awaiting arguments.
#[tokio::test]
async fn specialize_rejects_an_arity_mismatch() {
    let message = thrown_diagnostic(
        r#"
        function identity<T>(value: T) -> T { value }

        function main() -> string throws unknown {
            let descriptor = reflect.Package.current().functions().get("root.identity")
                ?? throw "identity not listed"
            let _ = descriptor.specialize([reflect.Type.of<int>(), reflect.Type.of<string>()]) catch (e) {
                reflect.errors.CompilationError => {
                    return e.diagnostics[0].code + "|" + e.diagnostics[0].message
                },
                _ => return "wrong error",
            }
            "specialize did not throw"
        }
        "#,
    )
    .await;
    assert_eq!(
        message,
        "E0169|cannot specialize generic function `identity`: it declares 1 type parameter, \
         but 2 type arguments were supplied"
    );
}

/// A declared interface bound is proved against the supplied value, using the
/// same prover the VM runs before entering a runtime-checked generic call.
#[tokio::test]
async fn specialize_rejects_a_bound_violation() {
    let message = thrown_diagnostic(
        r#"
        interface Shaped {
            function area(self) -> int throws never
        }

        class Square {
            side int
            implements Shaped {
                function area(self) -> int { self.side * self.side }
            }
        }

        class Loose { label string }

        function measure<T extends Shaped>(value: T) -> int { 0 }

        function main() -> string throws unknown {
            let descriptor = reflect.Package.current().functions().get("root.measure")
                ?? throw "measure not listed"
            let _ = descriptor.specialize([reflect.Type.of<Loose>()]) catch (e) {
                reflect.errors.CompilationError => {
                    return e.diagnostics[0].code + "|" + e.diagnostics[0].message
                },
                _ => return "wrong error",
            }
            "specialize did not throw"
        }
        "#,
    )
    .await;
    assert_eq!(
        message,
        "E0169|cannot specialize generic function `measure`: type argument `Loose` does not \
         satisfy the bound `T extends Shaped`"
    );
}

/// `baml.AnyClass` is compiler-derived rather than impl-driven (#4493), so it
/// gets its own row: a primitive supplied for an `AnyClass`-bounded parameter
/// must be refused by the same path.
#[tokio::test]
async fn specialize_rejects_an_any_class_bound_violation() {
    let message = thrown_diagnostic(
        r#"
        function inspect<T extends baml.AnyClass>(value: T) -> string { "" }

        function main() -> string throws unknown {
            let descriptor = reflect.Package.current().functions().get("root.inspect")
                ?? throw "inspect not listed"
            let _ = descriptor.specialize([reflect.Type.of<int>()]) catch (e) {
                reflect.errors.CompilationError => {
                    return e.diagnostics[0].code + "|" + e.diagnostics[0].message
                },
                _ => return "wrong error",
            }
            "specialize did not throw"
        }
        "#,
    )
    .await;
    assert_eq!(
        message,
        "E0169|cannot specialize generic function `inspect`: type argument `int` does not \
         satisfy the bound `T extends baml.AnyClass`"
    );
}

/// `is_generic` is the guard; specializing a concrete function says so.
#[tokio::test]
async fn specialize_rejects_a_non_generic_function() {
    let message = thrown_diagnostic(
        r#"
        function plain(value: string) -> string { value }

        function main() -> string throws unknown {
            let descriptor = reflect.Package.current().functions().get("root.plain")
                ?? throw "plain not listed"
            let _ = descriptor.specialize([reflect.Type.of<int>()]) catch (e) {
                reflect.errors.CompilationError => {
                    return e.diagnostics[0].code + "|" + e.diagnostics[0].message
                },
                _ => return "wrong error",
            }
            "specialize did not throw"
        }
        "#,
    )
    .await;
    assert_eq!(
        message,
        "E0169|function `plain` is not generic; there is nothing to specialize"
    );
}

/// An unspecialized generic descriptor has no realized function type — the
/// realized type family cannot spell a type variable — so a signature-shaped
/// read is a typed diagnostic, never the `unreachable!` it would have been.
#[tokio::test]
async fn unspecialized_signature_read_is_a_typed_diagnostic() {
    let message = thrown_diagnostic(
        r#"
        function identity<T>(value: T) -> T { value }

        function main() -> string throws unknown {
            let descriptor = reflect.Package.current().functions().get("root.identity")
                ?? throw "identity not listed"
            let _ = descriptor.return_type() catch (e) {
                reflect.errors.CompilationError => {
                    return e.diagnostics[0].code + "|" + e.diagnostics[0].message
                },
                _ => return "wrong error",
            }
            "return_type did not throw"
        }
        "#,
    )
    .await;
    assert_eq!(
        message,
        "E0165|generic function `identity` has no signature until it is specialized: its \
         parameter and return types still mention its own type parameters"
    );
}

/// A function type that reflection did not hand out carries no callable: it
/// reads as non-generic, extracts to null, and cannot be specialized.
#[tokio::test]
async fn a_plain_function_type_is_not_a_descriptor() {
    let message = thrown_diagnostic(
        r#"
        type Callback = (value: int) -> int throws never;

        function main() -> string throws unknown {
            let view = reflect.Type.of<Callback>().as_function() ?? throw "not a function type"
            if (view.is_generic()) { return "reported generic" } else { null }
            if (view.generic_params().length() != 0) { return "reported params" } else { null }
            if (view.get<Callback>() != null) { return "get returned a callable" } else { null }
            let _ = view.specialize([reflect.Type.of<int>()]) catch (e) {
                reflect.errors.CompilationError => {
                    return e.diagnostics[0].code + "|" + e.diagnostics[0].message
                },
                _ => return "wrong error",
            }
            "specialize did not throw"
        }
        "#,
    )
    .await;
    assert_eq!(
        message,
        "E0169|this function type is not a reflected function descriptor: only the entries of \
         `Package.functions()` carry the callable `specialize` needs"
    );
}

/// Extraction still enforces the caller's contract, exactly as
/// `Package.get_function` does — the descriptor route reuses that check.
#[tokio::test]
async fn descriptor_extraction_enforces_the_requested_contract() {
    let message = thrown_diagnostic(
        r#"
        type StringFn = (value: string) -> string throws never;

        function identity<T>(value: T) -> T { value }

        function main() -> string throws unknown {
            let descriptor = reflect.Package.current().functions().get("root.identity")
                ?? throw "identity not listed"
            let specialized = descriptor.specialize([reflect.Type.of<int>()])
            let _ = specialized.get<StringFn>() catch (e) {
                reflect.errors.CompilationError => {
                    return e.diagnostics[0].code
                },
                _ => return "wrong error",
            } else {
                return "get returned null"
            }
            "get did not throw"
        }
        "#,
    )
    .await;
    assert_eq!(message, "E0001");
}

/// A descriptor's callable is reachable *only* through the descriptor's own
/// `type` value. That edge is traced by `TypeValue::gc_edges`; open-coding the
/// walk at one of the collector's sites and missing it leaves the descriptor
/// pointing at freed or moved memory, which `get_object`'s unchecked deref
/// turns into undefined behaviour rather than a panic. Collect between every
/// step.
#[tokio::test]
async fn a_descriptor_survives_collection_between_every_step() {
    let output = baml_test!(
        r#"
        type IntFn = (value: int) -> int throws never;

        function identity<T>(value: T) -> T { value }

        function main() -> int throws unknown {
            let descriptor = reflect.Package.current().functions().get("root.identity")
                ?? throw "identity not listed"
            baml.sys.collect_garbage()
            let specialized = descriptor.specialize([reflect.Type.of<int>()])
            baml.sys.collect_garbage()
            let callable = specialized.get<IntFn>() ?? throw "no callable"
            baml.sys.collect_garbage()
            callable(7)
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(7)));
}

/// The other half of the same edge: a specialized callable holds the exact
/// `type` values it was specialized with, and its body reads one back through
/// `reflect.Type.of<T>()`. Those values carry their own package owner and definition
/// overlay, so a collection between specialization and call has to root and
/// forward all of them.
#[tokio::test]
async fn a_specialized_body_reads_its_own_type_across_collection() {
    let output = baml_test!(
        r#"
        function reflected<T>() -> reflect.Type { reflect.Type.of<T>() }

        function main() -> bool throws unknown {
            let schema = reflect.Package.compile({
                "schema.baml": "class ExtractedRecord { account string amount int }",
            })
            let record = schema.get_class("root.ExtractedRecord") ?? throw "missing ExtractedRecord"
            let minted = record.as_type()

            let specialized = (reflect.Package.current().functions().get("root.reflected")
                ?? throw "reflected not listed").specialize([minted])
            baml.sys.collect_garbage()
            let callable = specialized.get<baml.AnyFunction<Returns = reflect.Type, Throws = unknown>>()
                ?? throw "no callable"
            baml.sys.collect_garbage()
            let materialized = reflect.call_any<reflect.Type, unknown>(callable, {})
            baml.sys.collect_garbage()

            let view = materialized.as_class() ?? throw "materialized type is not a class"
            let field = view.fields().at(0) ?? throw "no fields"
            materialized == minted && field.name == "account"
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

/// Bounds declared *inside* a runtime-compiled package are local to it, so the
/// proof has to resolve the interface in the descriptor's world rather than
/// whoever happens to be calling `specialize`. Rooting it at the caller made
/// every such bound unprovable and rejected the conforming type too.
#[tokio::test]
async fn bounds_declared_in_a_runtime_package_are_provable() {
    let output = baml_test!(
        r##"
        function main() -> string throws unknown {
            let pkg = reflect.Package.compile({
                "main.baml": #"
interface Shaped {
  function area(self) -> int throws never
}

class Square {
  side int
  implements Shaped {
    function area(self) -> int { self.side * self.side }
  }
}

class Loose { label string }

function measure<T extends Shaped>(value: T) -> int { 0 }
"#
            })
            let descriptor = pkg.functions().get("root.measure") ?? throw "measure not listed"
            let square = pkg.get_class("root.Square") ?? throw "missing Square"
            let loose = pkg.get_class("root.Loose") ?? throw "missing Loose"

            let _accepted = descriptor.specialize([square.as_type()])
            let _ = descriptor.specialize([loose.as_type()]) catch (e) {
                reflect.errors.CompilationError => {
                    return "rejected:" + e.diagnostics[0].code
                },
                _ => return "wrong error",
            }
            "the non-conforming type was accepted"
        }
        "##
    );
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("rejected:E0169".into()))
    );
}

/// Specialization is not incremental, so a second `specialize` is an error —
/// but telling the caller the callable "is not generic" points at the wrong
/// mistake. It gets its own message.
#[tokio::test]
async fn respecializing_reports_that_it_is_already_bound() {
    let message = thrown_diagnostic(
        r#"
        function identity<T>(value: T) -> T { value }

        function main() -> string throws unknown {
            let descriptor = reflect.Package.current().functions().get("root.identity")
                ?? throw "identity not listed"
            let specialized = descriptor.specialize([reflect.Type.of<int>()])
            let _ = specialized.specialize([reflect.Type.of<string>()]) catch (e) {
                reflect.errors.CompilationError => {
                    return e.diagnostics[0].code + "|" + e.diagnostics[0].message
                },
                _ => return "wrong error",
            }
            "specialize did not throw"
        }
        "#,
    )
    .await;
    assert_eq!(
        message,
        "E0169|generic function `identity` is already specialized; every type parameter is bound"
    );
}

/// Exact values are reported **positionally**, off the callable's own
/// templates: only a position written as the type parameter itself reports the
/// caller's value. A parameter the author wrote as a concrete type keeps its
/// own reconstruction even when that type is structurally the one supplied —
/// matching on the realized type instead would hand an unrelated parameter the
/// caller's identity.
#[tokio::test]
async fn exact_values_are_reported_by_slot_not_by_shape() {
    let output = baml_test!(
        r##"
        function main() -> bool throws unknown {
            let pkg = reflect.Package.compile({
                "main.baml": #"
class Rec { label string }

function pair<T>(first: T, second: Rec) -> T { first }
"#
            })
            let descriptor = pkg.functions().get("root.pair") ?? throw "pair not listed"
            let rec = pkg.get_class("root.Rec") ?? throw "missing Rec"
            let minted = rec.as_type()

            let specialized = descriptor.specialize([minted])
            let first = specialized.params().at(0) ?? throw "no first parameter"
            let second = specialized.params().at(1) ?? throw "no second parameter"

            // `first` is written `T`; `second` is written `Rec`, which realizes
            // to the very same class without being the caller's type argument.
            first.type == minted
                && second.type != minted
                && specialized.return_type() == minted
        }
        "##
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}
