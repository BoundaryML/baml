//! Runtime semantics of BEP-062's AnyFunction slice: `reflect.AnyFunction`
//! coercion carried to runtime, `reflect.signature`, and `reflect.call_any`
//! (argument checking, callee defaults, error propagation).

use baml_tests::{
    baml_test,
    engine::{IndexMap, OptLevel, compile_source_with_opt, run_compiled},
    stdlib_prefix::{check_user_files, setup_test_db},
};
use bex_engine::BexExternalValue;
use bex_vm_types::{ConstValue, Instruction, Object};

#[tokio::test]
async fn call_any_infers_pins_from_function_value() {
    let output = baml_test!(
        r#"
        function plain(required: string) -> string throws never {
            required
        }

        function main() -> string throws never {
            reflect.call_any(plain, { "required": "hello" }) catch (e) {
                _ => "error"
            }
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::String("hello".into())));
}

#[tokio::test]
async fn call_any_inferred_and_explicit_class_returns_match() {
    let output = baml_test!(
        r#"
        class Result {
            value string
        }

        function make_result() -> Result throws never {
            Result { value: "hello" }
        }

        function inferred() -> Result throws never {
            reflect.call_any(make_result, {}) catch (e) {
                _ => Result { value: "inferred error" }
            }
        }

        function explicit() -> Result throws never {
            reflect.call_any<Result, never>(make_result, {}) catch (e) {
                _ => Result { value: "explicit error" }
            }
        }

        function main() -> string throws never {
            inferred().value + "|" + explicit().value
        }
        "#
    );
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("hello|hello".into()))
    );
}

#[tokio::test]
async fn call_any_inferred_and_explicit_list_returns_match() {
    let output = baml_test!(
        r#"
        function make_list() -> string[] throws never {
            ["alpha", "beta"]
        }

        function main() -> string throws never {
            let inferred = reflect.call_any(make_list, {}) catch (e) {
                _ => ["inferred error"]
            }
            let explicit = reflect.call_any<string[], never>(make_list, {}) catch (e) {
                _ => ["explicit error"]
            }
            inferred[0] + ":" + inferred[1] + "|" + explicit[0] + ":" + explicit[1]
        }
        "#
    );
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("alpha:beta|alpha:beta".into()))
    );
}

#[tokio::test]
async fn call_any_inferred_and_explicit_map_returns_match() {
    let output = baml_test!(
        r#"
        function make_map() -> map<string, int> throws never {
            let result: map<string, int> = { "alpha": 1, "beta": 2 }
            result
        }

        function inferred_fallback() -> map<string, int> throws never {
            let result: map<string, int> = { "alpha": -10, "beta": -20 }
            result
        }

        function explicit_fallback() -> map<string, int> throws never {
            let result: map<string, int> = { "alpha": -100, "beta": -200 }
            result
        }

        function inferred() -> map<string, int> throws never {
            reflect.call_any(make_map, {}) catch (e) {
                _ => inferred_fallback()
            }
        }

        function explicit() -> map<string, int> throws never {
            reflect.call_any<map<string, int>, never>(make_map, {}) catch (e) {
                _ => explicit_fallback()
            }
        }

        function main() -> map<string, int>[] throws never {
            [inferred(), explicit()]
        }
        "#
    );
    let Ok(BexExternalValue::Array { items, .. }) = output.result else {
        panic!("expected inferred and explicit maps");
    };
    assert_eq!(items.len(), 2);
    for item in items {
        let BexExternalValue::Map { entries, .. } = item else {
            panic!("expected map result");
        };
        assert_eq!(entries.get("alpha"), Some(&BexExternalValue::Int(1)));
        assert_eq!(entries.get("beta"), Some(&BexExternalValue::Int(2)));
    }
}

#[tokio::test]
async fn call_any_inferred_and_explicit_throws_match() {
    let output = baml_test!(
        r#"
        class CallError {
            message string
        }

        function fail() -> string throws CallError {
            throw CallError { message: "boom" }
        }

        function inferred() -> string throws never {
            reflect.call_any(fail, {}) catch (e) {
                CallError => "inferred:" + e.message,
                reflect.InvalidArgumentError => "inferred argument error",
                reflect.errors.CompilationError => "inferred compilation error",
            }
        }

        function explicit() -> string throws never {
            reflect.call_any<string, CallError>(fail, {}) catch (e) {
                CallError => "explicit:" + e.message,
                reflect.InvalidArgumentError => "explicit argument error",
                reflect.errors.CompilationError => "explicit compilation error",
            }
        }

        function main() -> string throws never {
            inferred() + "|" + explicit()
        }
        "#
    );
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String(
            "inferred:boom|explicit:boom".into()
        ))
    );
}

#[tokio::test]
async fn call_any_inferred_class_return_crosses_sys_op_and_host_boundaries() {
    let output = baml_test!(
        r#"
        class Result {
            value string
        }

        function main() -> Result throws never {
            reflect.call_any(baml.sap.parse<Result>, { "text": `{"value":"hello"}` }) catch (e) {
                _ => Result { value: "error" }
            }
        }
        "#
    );
    assert_eq!(
        output.result,
        Ok(BexExternalValue::instance(
            "user.Result",
            [("value", BexExternalValue::String("hello".into()))]
                .into_iter()
                .collect(),
        ))
    );
}

#[tokio::test]
async fn call_any_rejects_a_return_outside_inferred_r() {
    let mut program = compile_source_with_opt(
        r#"
        function lie() -> string throws never {
            "declared string"
        }

        function main() -> string throws never {
            reflect.call_any(lie, {}) catch_all (e) {
                reflect.InvalidArgumentError => {
                    return e.argument + "|" + e.expected.to_string() + "|" + e.got.to_string()
                },
                _ => return "unexpected error",
            }
            "mismatch was accepted"
        }
        "#,
        OptLevel::One,
    );

    // Preserve `lie`'s declared `string` signature while deliberately making
    // its bytecode return an `int`. This models a faulty dynamic/host callee
    // without allowing another boundary to reject the value first.
    let lie_idx = program
        .function_index("user.lie")
        .expect("user.lie should exist");
    let Object::Function(lie) = program
        .objects
        .get_mut(lie_idx)
        .expect("user.lie object should exist")
    else {
        panic!("user.lie should be a function");
    };
    lie.bytecode.instructions = vec![Instruction::LoadConst(0), Instruction::Return];
    lie.bytecode.constants = vec![ConstValue::Int(42)];
    lie.bytecode.compact = None;

    let output = run_compiled(program, "main", IndexMap::new(), false).await;
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String(
            "reflect.call_any return value|string|int".into()
        ))
    );
}

#[tokio::test]
async fn call_any_dispatches_named_args() {
    let output = baml_test!(
        r#"
        function add(x: int, y: int) -> int throws never {
            x + y
        }

        function main() -> int throws never {
            let f: reflect.AnyFunction<Returns = int, Throws = never> = add
            return reflect.call_any(f, { "x": 20, "y": 22 }) catch (e) {
                _ => -1
            }
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(42)));
}

#[tokio::test]
async fn call_any_absent_optional_fires_callee_default() {
    let output = baml_test!(
        r#"
        function scale(x: int, factor: int = x + 1) -> int throws never {
            x * factor
        }

        function main() -> int throws never {
            let f: reflect.AnyFunction<Returns = int, Throws = never> = scale
            // Absent: the callee's own (non-constant) default fires: 5 * 6.
            let defaulted = reflect.call_any(f, { "x": 5 }) catch (e) {
                _ => -1
            }
            // Present: the supplied value wins: 5 * 10.
            let supplied = reflect.call_any(f, { "x": 5, "factor": 10 }) catch (e) {
                _ => -1
            }
            return defaulted * 1000 + supplied
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(30050)));
}

#[tokio::test]
async fn call_any_rejects_missing_key_and_type_mismatches() {
    let output = baml_test!(
        r#"
        function greet(name: string, excited: bool = false) -> string throws never {
            return name
        }

        function main() -> int throws never {
            let f: reflect.AnyFunction<Returns = string, Throws = never> = greet
            let missing = reflect.call_any(f, {}) catch (e) {
                reflect.InvalidArgumentError => 1,
                reflect.errors.CompilationError => 100,
            }
            let ty = reflect.call_any(f, { "name": 42 }) catch (e) {
                reflect.InvalidArgumentError => 1,
                reflect.errors.CompilationError => 100,
            }
            let key = reflect.call_any(f, { "name": "x", "volume": 11 }) catch (e) {
                reflect.InvalidArgumentError => 1,
                reflect.errors.CompilationError => 100,
            }
            let opt_ty = reflect.call_any(f, { "name": "x", "excited": "yes" }) catch (e) {
                reflect.InvalidArgumentError => 1,
                reflect.errors.CompilationError => 100,
            }
            let failures = 0
            if missing is int {
                failures = failures + missing
            }
            if ty is int {
                failures = failures + ty
            }
            if key is int {
                failures = failures + key
            }
            if opt_ty is int {
                failures = failures + opt_ty
            }
            return failures
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(4)));
}

#[tokio::test]
async fn call_any_widens_int_for_float_param() {
    let output = baml_test!(
        r#"
        function scale(budget: float, factor: float? = null) -> float throws never {
            let f = 1.0
            if factor != null {
                f = factor
            }
            return budget * f
        }

        function takes_int(n: int) -> int throws never {
            return n
        }

        function main() -> int throws never {
            let g: reflect.AnyFunction<Returns = float, Throws = never> = scale
            // The one boundary conversion: an integral value widens for a
            // `float` parameter (JSON Schema's `number` admits integers)...
            let widened = reflect.call_any(g, { "budget": 150 }) catch (e) {
                reflect.InvalidArgumentError => -1.0,
                reflect.errors.CompilationError => -100.0,
            }
            // ...and for a `float?` parameter.
            let opt = reflect.call_any(g, { "budget": 2, "factor": 3 }) catch (e) {
                reflect.InvalidArgumentError => -1.0,
                reflect.errors.CompilationError => -100.0,
            }
            // Nothing else converts: a float does not narrow to `int`, and a
            // numeric string does not parse to `float`.
            let h: reflect.AnyFunction<Returns = int, Throws = never> = takes_int
            let narrowed = reflect.call_any(h, { "n": 1.5 }) catch (e) {
                reflect.InvalidArgumentError => -1,
                reflect.errors.CompilationError => -100,
            }
            let stringy = reflect.call_any(g, { "budget": "150" }) catch (e) {
                reflect.InvalidArgumentError => -2.0,
                reflect.errors.CompilationError => -100.0,
            }
            // Widening is lossless-only: 2^53 is the last exactly
            // representable integer and widens; 2^53 + 1 would silently
            // round, so it stays an InvalidArgumentError.
            let exact = reflect.call_any(g, { "budget": 9007199254740992 }) catch (e) {
                reflect.InvalidArgumentError => -1.0,
                reflect.errors.CompilationError => -100.0,
            }
            let lossy = reflect.call_any(g, { "budget": 9007199254740993 }) catch (e) {
                reflect.InvalidArgumentError => -3.0,
                reflect.errors.CompilationError => -100.0,
            }
            let ok = 0
            if widened == 150.0 {
                ok = ok + 1
            }
            if opt == 6.0 {
                ok = ok + 1
            }
            if narrowed == -1 {
                ok = ok + 1
            }
            if stringy == -2.0 {
                ok = ok + 1
            }
            if exact == 9007199254740992.0 {
                ok = ok + 1
            }
            if lossy == -3.0 {
                ok = ok + 1
            }
            return ok
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(6)));
}

#[tokio::test]
async fn call_any_invalid_argument_error_carries_types() {
    let output = baml_test!(
        r#"
        function greet(name: string) -> string throws never {
            return name
        }

        function main() -> string throws never {
            let f: reflect.AnyFunction<Returns = string, Throws = never> = greet
            let bad_value = reflect.call_any(f, { "name": 42 }) catch (e) {
                reflect.InvalidArgumentError => e.argument + ":" + e.expected.to_string() + "|" + e.got.to_string(),
                reflect.errors.CompilationError => "unexpected-compilation-error",
            }
            let missing = reflect.call_any(f, {}) catch (e) {
                reflect.InvalidArgumentError => e.argument + ":" + e.expected.to_string() + "|" + e.got.to_string(),
                reflect.errors.CompilationError => "unexpected-compilation-error",
            }
            let out = "?"
            if bad_value is string {
                out = bad_value
            }
            if missing is string {
                out = out + "/" + missing
            }
            return out
        }
        "#
    );
    assert_eq!(
        output.result,
        // The reconstructed runtime type of the value `42` is its base type;
        // a missing required parameter reports its type against `never`.
        Ok(BexExternalValue::String(
            "name:string|int/name:string|never".into()
        ))
    );
}

#[tokio::test]
async fn call_any_propagates_callee_typed_throw() {
    let output = baml_test!(
        r#"
        class ToolError {
            message string
        }

        function fail_search(q: string) -> string throws ToolError {
            throw ToolError { message: "no results for " + q }
        }

        function main() -> string throws never {
            let f: reflect.AnyFunction<Returns = string, Throws = ToolError> = fail_search
            // Exhaustive without a wildcard: all declared channels are named.
            return reflect.call_any(f, { "q": "cats" }) catch (e) {
                ToolError => e.message,
                reflect.InvalidArgumentError => "iae",
                reflect.errors.CompilationError => "compilation-error",
            }
        }
        "#
    );
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("no results for cats".into()))
    );
}

#[tokio::test]
async fn call_any_heterogeneous_tool_map_dispatch() {
    let output = baml_test!(
        r#"
        class ToolError {
            message string
        }

        function shout(s: string) -> string throws never {
            return s + "!"
        }

        function fail(s: string) -> string throws ToolError {
            throw ToolError { message: s }
        }

        function dispatch(tools: map<string, reflect.AnyFunction<Returns = string, Throws = ToolError>>, name: string, args: map<string, unknown>) -> string throws never {
            let f = tools.get(name)
            if f is null {
                return "no-such-tool"
            }
            return reflect.call_any(f, args) catch (e) {
                ToolError => "err:" + e.message,
                reflect.InvalidArgumentError => "iae",
                reflect.errors.CompilationError => "compilation-error",
            }
        }

        function main() -> string throws never {
            let tools: map<string, reflect.AnyFunction<Returns = string, Throws = ToolError>> = {
                "shout": shout,
                "fail": fail,
            }
            return dispatch(tools, "shout", { "s": "hi" }) + "/" + dispatch(tools, "fail", { "s": "down" }) + "/" + dispatch(tools, "nope", {})
        }
        "#
    );
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("hi!/err:down/no-such-tool".into()))
    );
}

#[tokio::test]
async fn call_any_lambda_callee() {
    let output = baml_test!(
        r#"
        function main() -> int throws never {
            let double = (x: int) -> int throws never {
                x * 2
            }
            let f: reflect.AnyFunction<Returns = int, Throws = never> = double
            return reflect.call_any(f, { "x": 21 }) catch (e) {
                _ => -1
            }
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(42)));
}

#[tokio::test]
async fn call_any_bound_method_callee() {
    let output = baml_test!(
        r#"
        class Counter {
            base int

            function bump(self, amount: int) -> int throws never {
                self.base + amount
            }
        }

        function main() -> int throws never {
            let c = Counter { base: 40 }
            let m: reflect.AnyFunction<Returns = int, Throws = never> = c.bump
            return reflect.call_any(m, { "amount": 2 }) catch (e) {
                _ => -1
            }
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(42)));
}

#[tokio::test]
async fn signature_reports_types_and_param_split() {
    let output = baml_test!(
        r#"
        class ToolError {
            message string
        }

        /// Searches the index.
        function search(q: string, limit: int = 10) -> string[] throws ToolError {
            if q == "boom" {
                throw ToolError { message: "x" }
            }
            return [q]
        }

        function main() -> string throws never {
            let f: reflect.AnyFunction = search
            let sig = reflect.signature(f)
            let limit = sig.opts.get("limit")
            let limit_str = "absent"
            if limit != null {
                limit_str = limit.type.to_string()
            }
            let fn_name = "unnamed"
            let sn = sig.name
            if sn != null {
                fn_name = sn
            }
            let q_name = sig.args[0].name
            let doc = "no-doc"
            let d = sig.docstring
            if d != null {
                doc = d
            }
            return fn_name + "|" + sig.returns.to_string() + "|" + sig.errors.to_string()
                + "|" + sig.args.length().to_string() + "|" + sig.args[0].type.to_string()
                + "|" + q_name + "|" + limit_str + "|" + doc
        }
        "#
    );
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String(
            "search|string[]|ToolError|1|string|q|int|Searches the index.".into()
        ))
    );
}

#[tokio::test]
async fn signature_bound_method_drops_receiver() {
    let output = baml_test!(
        r#"
        class Counter {
            base int

            function bump(self, amount: int) -> int throws never {
                self.base + amount
            }
        }

        function main() -> string throws never {
            let c = Counter { base: 40 }
            let m: reflect.AnyFunction = c.bump
            let sig = reflect.signature(m)
            let method_name = "unnamed"
            let n = sig.name
            if n != null {
                method_name = n
            }
            // A lambda has no source-level name, so it reports none.
            let lambda_name = "some"
            if reflect.signature((x: int) -> int throws never { x }).name == null {
                lambda_name = "null"
            }
            // Also anchors the non-throwing spelling: `errors` reads `never`.
            return method_name + "|" + lambda_name + "|" + sig.args.length().to_string()
                + "|" + sig.returns.to_string() + "|" + sig.errors.to_string()
        }
        "#
    );
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("bump|null|1|int|never".into()))
    );
}

#[tokio::test]
async fn instantiated_generic_function_reflects_precisely_and_dispatches() {
    let output = baml_test!(
        r#"
        function ident<T>(x: T) -> T throws never {
            return x
        }

        function main() -> string throws never {
            // An instantiated generic callable carries its realized frame, so its
            // signature reconstructs at that instantiation rather than coarsely.
            let f: reflect.AnyFunction = ident<int>
            let sig = reflect.signature(f)
            let r = reflect.call_any(f, { "x": 42 }) catch (e) {
                _ => -1
            }
            let called = "no"
            if r is int {
                called = r.to_string()
            }
            return sig.returns.to_string() + "|" + sig.args.length().to_string() + "|" + called
        }
        "#
    );
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("int|1|42".into()))
    );
}

#[tokio::test]
async fn call_any_reports_unspecialized_generic_function() {
    let output = baml_test!(
        r#"
        function ident<T>(x: T) -> T throws never {
            return x
        }

        function main() -> string throws unknown {
            let f: reflect.AnyFunction = ident
            let _ = reflect.call_any(f, { "x": 42 }) catch (e) {
                reflect.errors.CompilationError => {
                    return e.diagnostics[0].code + "|" + e.diagnostics[0].message
                },
                _ => throw e,
            }
            return "generic call unexpectedly succeeded"
        }
        "#
    );
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String(
            "E0165|generic function `ident` cannot be extracted through reflection: its signature still mentions its own type parameters".into()
        ))
    );
}

#[tokio::test]
async fn pinned_call_any_declares_unspecialized_generic_compilation_error() {
    let output = baml_test!(
        r#"
        function ident<T>(x: T) -> T throws never {
            return x
        }

        function main() -> string throws never {
            let f: reflect.AnyFunction<Returns = string, Throws = never> = ident
            let _ = reflect.call_any(f, { "x": "value" }) catch (e) {
                reflect.errors.CompilationError => {
                    return e.diagnostics[0].code + "|" + e.diagnostics[0].message
                },
                _ => return "wrong error",
            }
            return "generic call unexpectedly succeeded"
        }
        "#
    );
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String(
            "E0165|generic function `ident` cannot be extracted through reflection: its signature still mentions its own type parameters".into()
        ))
    );
}

/// An *uninstantiated* generic reference — no turbofish and nothing to infer
/// from — is a compile error: a callable value must carry a realized frame, so
/// there is no such thing as a half-generic one to reflect on. Ignored until
/// that diagnostic exists; today the reference is accepted and only fails later,
/// at `reflect.signature`, with a misleading "expects a function value".
#[ignore = "pending the uninstantiated-generic-reference diagnostic"]
#[tokio::test]
async fn uninstantiated_generic_function_reference_is_a_compile_error() {
    let output = baml_test!(
        r#"
        function ident<T>(x: T) -> T throws never {
            return x
        }

        function main() -> string throws never {
            let f: reflect.AnyFunction = ident
            return "unreachable"
        }
        "#
    );
    assert!(
        output.result.is_err(),
        "expected a compile error naming the uninferable type parameter, got {:?}",
        output.result
    );
}

#[tokio::test]
async fn signature_object_literal_construction() {
    let output = baml_test!(
        r#"
        function main() -> string throws never {
            // Manual Signature construction through the plain class literal.
            // (No keyword-named fields: the error channel field is `errors`.)
            let manual = reflect.Signature {
                args: [],
                opts: {},
                returns: reflect.Type.of<int>(),
                errors: reflect.Type.of<never>(),
                docstring: null,
            }
            return manual.returns.to_string() + "|" + manual.errors.to_string()
        }
        "#
    );
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("int|never".into()))
    );
}

// ---------------------------------------------------------------------------
// Runtime-minted enums through offline LLM companions.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn unreflect_reifies_the_runtime_type_argument() {
    let output = baml_test!(
        r#"
        function inspect<T>() -> string throws never {
            return reflect.Type.of<T>().to_string()
        }

        function main() -> string throws reflect.errors.CompilationError {
            let t = reflect.enum.new("Category", ["RED", "BLUE"])
            return inspect<unreflect(t)>()
        }
        "#
    );
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("Category".into()))
    );
}

#[tokio::test]
async fn runtime_enum_renders_and_alias_round_trips_through_sap() {
    let output = baml_test!(
        r##"
        client TestClient = openai.ResponsesClient.new(
    model = "gpt-4o-mini",
    api_key = "test-key",
    base_url = "http://localhost:1234",
);

        function Classify<T>(input: string) -> T {
            client: TestClient
            prompt: `Choose a category for ${input}.\n${ctx.output_format()}`
        }

        function main() -> string {
            let t = reflect.enum.new("Category", [
                reflect.enum.value("RED", alias = "k7", description = "warm"),
                reflect.enum.value("BLUE", description = "cool"),
            ])
            let prompt = Classify$render_prompt<unreflect(t)>("sample").text()
            let parsed = Classify$parse<unreflect(t)>(`"k7"`)
            return prompt + "\n<PARSED>" + reflect.enum.get_value(parsed)
        }
        "##
    );

    let BexExternalValue::String(result) = output
        .result
        .expect("runtime enum render and parse should succeed")
    else {
        panic!("expected string result")
    };
    assert!(
        result.contains("Category"),
        "schema omitted enum name: {result}"
    );
    assert!(
        result.contains("k7"),
        "schema omitted serialized alias: {result}"
    );
    assert!(
        result.contains("BLUE"),
        "schema omitted ordinary value: {result}"
    );
    assert!(
        result.ends_with("<PARSED>RED"),
        "alias must parse back to the source value name: {result}"
    );
}

#[tokio::test]
async fn nested_unreflect_runtime_type_renders_through_a_generic_wrapper() {
    let output = baml_test!(
        r##"
client TestClient = openai.ResponsesClient.new(
    model = "gpt-4o-mini",
    api_key = "test-key",
    base_url = "http://localhost:1234",
)

class Wrapper<T> {
    value T
}

function Extract<T>(input: string) -> T {
    client: TestClient
    prompt: `Extract ${input}.\n${ctx.output_format()}`
}

function main() -> string {
    let runtime_class = reflect.class.new("RuntimeTranscript", {
        "speaker": reflect.Type.of<string>(),
        "words": reflect.Type.of<string[]>(),
    })
    Extract$render_prompt<Wrapper<unreflect(runtime_class.as_type())>>("sample").text()
}
"##
    );

    let BexExternalValue::String(prompt) = output
        .result
        .expect("nested runtime type should render without executing a model call")
    else {
        panic!("expected rendered prompt text")
    };
    assert!(
        prompt.contains("value:") && prompt.contains("speaker") && prompt.contains("words"),
        "missing static wrapper or nested runtime class schema: {prompt}",
    );
}

#[tokio::test]
async fn runtime_enum_identity_and_metadata_are_preserved() {
    let output = baml_test!(
        r##"
        client TestClient = openai.ResponsesClient.new(
    model = "gpt-4o-mini",
    api_key = "test-key",
    base_url = "http://localhost:1234",
);

        function Classify<T>(input: string) -> T {
            client: TestClient
            prompt: `Choose a category for ${input}.\n${ctx.output_format()}`
        }

        function main() -> string {
            // Widen explicitly (`as_type`) to select `reflect.Type.meta(...)`,
            // rather than the enum-kind view's zero-argument metadata reader.
            let left: reflect.Type = reflect.enum.new("Category", ["RED", "BLUE"]).as_type()
            let right: reflect.Type = reflect.enum.new("Category", ["RED", "BLUE"]).as_type()
            let left_prompt = Classify$render_prompt<unreflect(left)>("sample").text()
            let right_prompt = Classify$render_prompt<unreflect(right)>("sample").text()
            let tagged = left.meta(
                alias = "category_code",
                description = "A generated category",
                docstring = "runtime docs",
                other = { "owner": "tests" },
            )
            let owner = tagged.other.get("owner")
            return (left != right).to_string()
                + "|" + (left_prompt == right_prompt).to_string()
                + "|" + (tagged.ty == left).to_string()
                + "|" + (tagged.alias ?? "null")
                + "|" + (tagged.description ?? "null")
                + "|" + (tagged.docstring ?? "null")
                + "|" + (owner ?? "null")
        }
        "##
    );

    assert_eq!(
        output.result,
        Ok(BexExternalValue::String(
            "true|true|true|category_code|A generated category|runtime docs|tests".into()
        ))
    );
}

#[tokio::test]
async fn duplicate_runtime_enum_value_uses_compiler_diagnostic() {
    let output = baml_test!(
        r#"
        function main() -> string throws never {
            let result = reflect.enum.new("Category", ["RED", "RED"]) catch (e) {
                reflect.errors.CompilationError => e.diagnostics[0].code + "|" + e.diagnostics[0].message
            }
            if result is string {
                return result
            }
            return "constructor did not throw"
        }
        "#
    );

    let BexExternalValue::String(result) = output
        .result
        .expect("duplicate definition should be catchable")
    else {
        panic!("expected string result")
    };
    assert!(
        result.starts_with("E0012|"),
        "wrong diagnostic code: {result}"
    );
    assert!(
        result.contains("duplicate variant `Category.RED`"),
        "wrong diagnostic message: {result}"
    );
}

#[tokio::test]
async fn empty_runtime_enum_fails_at_the_render_boundary() {
    let output = baml_test!(
        r##"
        client TestClient = openai.ResponsesClient.new(
    model = "gpt-4o-mini",
    api_key = "test-key",
    base_url = "http://localhost:1234",
);

        function Classify<T>(input: string) -> T {
            client: TestClient
            prompt: `Choose a category for ${input}.\n${ctx.output_format()}`
        }

        function main() -> string throws never {
            let t = reflect.enum.new("Category", []) catch (e) {
                _ => return "constructor threw"
            }
            let rendered = Classify$render_prompt<unreflect(t)>("sample") catch (e) {
                reflect.errors.CompilationError => e.diagnostics[0].code + "|" + e.diagnostics[0].message,
                _ => "wrong render error",
            }
            if rendered is string {
                return rendered
            }
            return "render did not throw"
        }
        "##
    );

    let BexExternalValue::String(result) = output
        .result
        .expect("empty runtime enum render failure should be catchable")
    else {
        panic!("expected string result")
    };
    assert!(result.starts_with("E0159|"), "wrong diagnostic: {result}");
    assert!(
        result.contains("empty enum `Category` cannot be rendered"),
        "wrong diagnostic message: {result}"
    );
}

#[test]
fn runtime_type_arguments_are_rejected_on_streaming_companions() {
    let db = setup_test_db(
        r##"
        client TestClient = openai.ResponsesClient.new(
    model = "gpt-4o-mini",
    api_key = "test-key",
);

        function Classify<T>(input: string) -> T {
            client: TestClient
            prompt: `${input} ${ctx.output_format()}`
        }

        function main() -> null {
            let t = reflect.enum.new("Category", ["RED"])
            Classify$stream<unreflect(t)>("sample")
            return null
        }
        "##,
    );
    let diagnostics = check_user_files(&db);
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains(
                "runtime type arguments are not supported on streaming call `Classify$stream`"
            )),
        "missing streaming firewall diagnostic: {diagnostics:#?}"
    );
}

/// B-1582 item 3: a generic LLM function's `$render_prompt` companion has a
/// signature free of `T` (it takes the parent's value arguments and returns an
/// `ai.Prompt`), so it reconstructs and `Package.get_function` used to hand it
/// out. Its *body* still materializes `T` for the output-format schema, and
/// entering it with an empty frame died as a VM internal error ("template
/// references frame type-arg slot 0 but the frame has 0 type args"). Until
/// reflection can supply type arguments, that is a normal E0165 — refused at
/// extraction, through the `AnyFunction` contract as well as a concrete one.
#[tokio::test]
async fn get_function_refuses_an_unspecialized_generic_through_any_function() {
    let output = baml_test!(
        r#"
        client TestClient = openai.ResponsesClient.new(
            model = "gpt-4o-mini",
            api_key = "test-key",
            base_url = "http://localhost:1234",
        );

        type AnyCallable = reflect.AnyFunction<Returns = unknown, Throws = unknown>;

        function GenericList<T>(topic: string) -> T[] {
            client: TestClient
            prompt: `
                Return an empty list of ${topic}.
                ${ctx.output_format()}
            `
        }

        function main() -> string throws never {
            let package = reflect.Package.current()
            let callable: AnyCallable = package.get_function<AnyCallable>(
                "GenericList$render_prompt",
            ) catch (e) {
                reflect.errors.CompilationError => {
                    return e.diagnostics[0].code + "|" + e.diagnostics[0].message
                },
                _ => return "wrong error",
            } else {
                return "get_function returned null"
            }
            // Unreachable: extraction throws above. `reflect.call_any` keeps the
            // same check for any callable that reaches it by another door.
            reflect.call_any<unknown, unknown>(callable, { "topic": "items" }) catch_all (e) {
                _ => return "call_any threw",
            }
            "get_function did not throw"
        }
        "#
    );
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String(
            "E0165|generic function `GenericList$render_prompt` cannot be invoked through \
             reflection: its body needs type arguments"
                .into()
        ))
    );
}

/// The floor is gated on the callable actually being an under-supplied generic:
/// a non-generic companion of an ordinary LLM function still invokes.
#[tokio::test]
async fn call_any_still_invokes_a_non_generic_companion() {
    let output = baml_test!(
        r#"
        client TestClient = openai.ResponsesClient.new(
            model = "gpt-4o-mini",
            api_key = "test-key",
            base_url = "http://localhost:1234",
        );

        type AnyCallable = reflect.AnyFunction<Returns = unknown, Throws = unknown>;

        function Plain(topic: string) -> string {
            client: TestClient
            prompt: `
                Say ${topic}.
                ${ctx.output_format()}
            `
        }

        function main() -> bool throws unknown {
            let package = reflect.Package.current()
            let callable = package.get_function<AnyCallable>("Plain$render_prompt")
                ?? throw "expected the companion"
            let rendered = reflect.call_any<unknown, unknown>(callable, { "topic": "hello" })
            rendered is ai.Prompt
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

/// The floor has to sit at *extraction*, not only at `reflect.call_any`: a
/// caller can ask for the companion through an ordinary function-type contract
/// and then call the value directly, which enters the body with an empty frame
/// and fails as a VM internal error no `catch` can see. `Package.get_function`
/// refuses it while the caller still has a diagnostic channel.
#[tokio::test]
async fn get_function_refuses_an_unspecialized_generic_companion() {
    let output = baml_test!(
        r#"
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
                ${ctx.output_format()}
            `
        }

        function main() -> string throws never {
            let package = reflect.Package.current()
            let callable: PromptFn = package.get_function<PromptFn>(
                "GenericList$render_prompt",
            ) catch (e) {
                reflect.errors.CompilationError => {
                    return e.diagnostics[0].code + "|" + e.diagnostics[0].message
                },
                _ => return "wrong error",
            } else {
                return "get_function returned null"
            }
            // Unreachable: the extraction above throws. Calling it directly is
            // what used to die inside the body as an uncatchable internal error.
            callable("items") catch_all (e) {
                _ => return "direct call threw",
            }
            "get_function did not throw"
        }
        "#
    );
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String(
            "E0165|generic function `GenericList$render_prompt` cannot be invoked through \
             reflection: its body needs type arguments"
                .into()
        ))
    );
}

/// Extraction of an ordinary non-generic companion through the same contract
/// still works — the refusal is gated on the callable being an under-supplied
/// generic whose body needs the missing arguments.
#[tokio::test]
async fn get_function_still_extracts_a_non_generic_companion() {
    let output = baml_test!(
        r#"
        client TestClient = openai.ResponsesClient.new(
            model = "gpt-4o-mini",
            api_key = "test-key",
            base_url = "http://localhost:1234",
        );

        type PromptFn = (topic: string) -> ai.Prompt throws unknown;

        function Plain(topic: string) -> string {
            client: TestClient
            prompt: `
                Say ${topic}.
                ${ctx.output_format()}
            `
        }

        function main() -> bool throws unknown {
            let package = reflect.Package.current()
            let callable = package.get_function<PromptFn>("Plain$render_prompt")
                ?? throw "expected the companion"
            let rendered = callable("hello")
            rendered.text().length() > 0
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}
