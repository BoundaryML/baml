//! Runtime semantics of BEP-062's AnyFunction slice: `baml.AnyFunction`
//! coercion carried to runtime, `reflect.signature`, and `reflect.call_any`
//! (argument checking, callee defaults, error propagation).

use baml_tests::baml_test;
use bex_engine::BexExternalValue;

#[tokio::test]
async fn call_any_dispatches_named_args() {
    let output = baml_test!(
        r#"
        function add(x: int, y: int) -> int throws never {
            x + y
        }

        function main() -> int throws never {
            let f: baml.AnyFunction<Returns = int, Throws = never> = add
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
            let f: baml.AnyFunction<Returns = int, Throws = never> = scale
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
            let f: baml.AnyFunction<Returns = string, Throws = never> = greet
            let missing = reflect.call_any(f, {}) catch (e) {
                reflect.InvalidArgumentError => 1
            }
            let ty = reflect.call_any(f, { "name": 42 }) catch (e) {
                reflect.InvalidArgumentError => 1
            }
            let key = reflect.call_any(f, { "name": "x", "volume": 11 }) catch (e) {
                reflect.InvalidArgumentError => 1
            }
            let opt_ty = reflect.call_any(f, { "name": "x", "excited": "yes" }) catch (e) {
                reflect.InvalidArgumentError => 1
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
            let g: baml.AnyFunction<Returns = float, Throws = never> = scale
            // The one boundary conversion: an integral value widens for a
            // `float` parameter (JSON Schema's `number` admits integers)...
            let widened = reflect.call_any(g, { "budget": 150 }) catch (e) {
                reflect.InvalidArgumentError => -1.0
            }
            // ...and for a `float?` parameter.
            let opt = reflect.call_any(g, { "budget": 2, "factor": 3 }) catch (e) {
                reflect.InvalidArgumentError => -1.0
            }
            // Nothing else converts: a float does not narrow to `int`, and a
            // numeric string does not parse to `float`.
            let h: baml.AnyFunction<Returns = int, Throws = never> = takes_int
            let narrowed = reflect.call_any(h, { "n": 1.5 }) catch (e) {
                reflect.InvalidArgumentError => -1
            }
            let stringy = reflect.call_any(g, { "budget": "150" }) catch (e) {
                reflect.InvalidArgumentError => -2.0
            }
            // Widening is lossless-only: 2^53 is the last exactly
            // representable integer and widens; 2^53 + 1 would silently
            // round, so it stays an InvalidArgumentError.
            let exact = reflect.call_any(g, { "budget": 9007199254740992 }) catch (e) {
                reflect.InvalidArgumentError => -1.0
            }
            let lossy = reflect.call_any(g, { "budget": 9007199254740993 }) catch (e) {
                reflect.InvalidArgumentError => -3.0
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
            let f: baml.AnyFunction<Returns = string, Throws = never> = greet
            let bad_value = reflect.call_any(f, { "name": 42 }) catch (e) {
                reflect.InvalidArgumentError => e.argument + ":" + e.expected.to_string() + "|" + e.got.to_string()
            }
            let missing = reflect.call_any(f, {}) catch (e) {
                reflect.InvalidArgumentError => e.argument + ":" + e.expected.to_string() + "|" + e.got.to_string()
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
            let f: baml.AnyFunction<Returns = string, Throws = ToolError> = fail_search
            // Exhaustive without a wildcard: the channel is exactly
            // ToolError | reflect.InvalidArgumentError.
            return reflect.call_any(f, { "q": "cats" }) catch (e) {
                ToolError => e.message,
                reflect.InvalidArgumentError => "iae"
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

        function dispatch(tools: map<string, baml.AnyFunction<Returns = string, Throws = ToolError>>, name: string, args: map<string, unknown>) -> string throws never {
            let f = tools.get(name)
            if f is null {
                return "no-such-tool"
            }
            return reflect.call_any(f, args) catch (e) {
                ToolError => "err:" + e.message,
                reflect.InvalidArgumentError => "iae"
            }
        }

        function main() -> string throws never {
            let tools: map<string, baml.AnyFunction<Returns = string, Throws = ToolError>> = {
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
            let f: baml.AnyFunction<Returns = int, Throws = never> = double
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
            let m: baml.AnyFunction<Returns = int, Throws = never> = c.bump
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
            let f: baml.AnyFunction = search
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
            let m: baml.AnyFunction = c.bump
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
            let f: baml.AnyFunction = ident<int>
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
            let f: baml.AnyFunction = ident
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
                returns: reflect.type_of<int>(),
                errors: reflect.type_of<never>(),
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
