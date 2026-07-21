//! Runtime semantics of BEP-062's AnyFunction slice: `baml.AnyFunction`
//! coercion carried to runtime, `reflect.signature`, and `reflect.call_any`
//! (argument checking, callee defaults, error propagation).

use baml_tests::baml_test;
use bex_engine::BexExternalValue;

#[tokio::test]
async fn call_any_dispatches_positional_args() {
    let output = baml_test!(
        r#"
        function add(x: int, y: int) -> int throws never {
            x + y
        }

        function main() -> int throws never {
            let f: baml.AnyFunction<Returns = int, Throws = never> = add
            return reflect.call_any(f, [20, 22]) catch (e) {
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
            let defaulted = reflect.call_any(f, [5]) catch (e) {
                _ => -1
            }
            // Present: the supplied value wins: 5 * 10.
            let supplied = reflect.call_any(f, [5], opts = { "factor": 10 }) catch (e) {
                _ => -1
            }
            return defaulted * 1000 + supplied
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(30050)));
}

#[tokio::test]
async fn call_any_rejects_bad_arity_type_and_key() {
    let output = baml_test!(
        r#"
        function greet(name: string, excited: bool = false) -> string throws never {
            return name
        }

        function main() -> int throws never {
            let f: baml.AnyFunction<Returns = string, Throws = never> = greet
            let arity = reflect.call_any(f, []) catch (e) {
                reflect.InvalidArgumentError => 1
            }
            let ty = reflect.call_any(f, [42]) catch (e) {
                reflect.InvalidArgumentError => 1
            }
            let key = reflect.call_any(f, ["x"], opts = { "volume": 11 }) catch (e) {
                reflect.InvalidArgumentError => 1
            }
            let opt_ty = reflect.call_any(f, ["x"], opts = { "excited": "yes" }) catch (e) {
                reflect.InvalidArgumentError => 1
            }
            let failures = 0
            if arity is int {
                failures = failures + arity
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
async fn call_any_invalid_argument_error_carries_types() {
    let output = baml_test!(
        r#"
        function greet(name: string) -> string throws never {
            return name
        }

        function main() -> string throws never {
            let f: baml.AnyFunction<Returns = string, Throws = never> = greet
            let out = reflect.call_any(f, [42]) catch (e) {
                reflect.InvalidArgumentError => e.expected.to_string() + "|" + e.got.to_string()
            }
            if out is string {
                return out
            }
            return "not-a-string"
        }
        "#
    );
    assert_eq!(
        output.result,
        // The reconstructed runtime type of the value `42` is its base type.
        Ok(BexExternalValue::String("string|int".into()))
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
            return reflect.call_any(f, ["cats"]) catch (e) {
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

        function dispatch(tools: map<string, baml.AnyFunction<Returns = string, Throws = ToolError>>, name: string, arg: string) -> string throws never {
            let f = tools.get(name)
            if f is null {
                return "no-such-tool"
            }
            return reflect.call_any(f, [arg]) catch (e) {
                ToolError => "err:" + e.message,
                reflect.InvalidArgumentError => "iae"
            }
        }

        function main() -> string throws never {
            let tools: map<string, baml.AnyFunction<Returns = string, Throws = ToolError>> = {
                "shout": shout,
                "fail": fail,
            }
            return dispatch(tools, "shout", "hi") + "/" + dispatch(tools, "fail", "down") + "/" + dispatch(tools, "nope", "x")
        }
        "#
    );
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("hi!/err:down/no-such-tool".into()))
    );
}

#[tokio::test]
async fn call_any_on_bare_any_function_returns_unknown() {
    let output = baml_test!(
        r#"
        function add(x: int, y: int) -> int throws never {
            x + y
        }

        function main() -> int throws never {
            let f: baml.AnyFunction = add
            // Bare pins: result is `unknown`, channel is `unknown` (covers
            // InvalidArgumentError); narrow the result back with `is`.
            let r = reflect.call_any(f, [20, 22]) catch (e) {
                _ => -1
            }
            if r is int {
                return r
            }
            return -2
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(42)));
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
            return reflect.call_any(f, [21]) catch (e) {
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
            return reflect.call_any(m, [2]) catch (e) {
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
            let q_name = sig.args[0].name
            let doc = "no-doc"
            let d = sig.docstring
            if d != null {
                doc = d
            }
            return sig.returns.to_string() + "|" + sig.errors.to_string()
                + "|" + sig.args.length().to_string() + "|" + sig.args[0].type.to_string()
                + "|" + q_name + "|" + limit_str + "|" + doc
        }
        "#
    );
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String(
            "string[]|ToolError|1|string|q|int|Searches the index.".into()
        ))
    );
}

#[tokio::test]
async fn signature_non_throwing_reports_never() {
    let output = baml_test!(
        r#"
        function add(x: int, y: int) -> int throws never {
            x + y
        }

        function main() -> string throws never {
            let f: baml.AnyFunction = add
            return reflect.signature(f).errors.to_string()
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::String("never".into())));
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
            return sig.args.length().to_string() + "|" + sig.returns.to_string()
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::String("1|int".into())));
}

#[tokio::test]
async fn generic_function_reflects_coarsely_and_dispatches() {
    let output = baml_test!(
        r#"
        function ident<T>(x: T) -> T throws never {
            return x
        }

        function main() -> string throws never {
            let f: baml.AnyFunction = ident
            // A generic callable's unresolved slots erase to `unknown` rather
            // than refusing reflection, and call_any still dispatches.
            let sig = reflect.signature(f)
            let r = reflect.call_any(f, [42]) catch (e) {
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
        Ok(BexExternalValue::String("unknown|1|42".into()))
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
