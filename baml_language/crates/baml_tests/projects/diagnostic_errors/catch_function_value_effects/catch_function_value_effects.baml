// A `catch` after a call that takes a function VALUE (not a lambda written at
// the call). The call throws what the type of the value says, in the place of
// the callee's effect parameter. Each function below reads the caught value
// in a way that is an error, so the snapshot shows the facts that the `catch`
// has. Before the fix every one of them was `never`.

function relay<T, E>(body: () -> T throws E) -> T throws E {
    body()
}

// Facts: `string`.
function relay_of_function_value(body: () -> int throws string) -> int {
    relay(body) catch_all (error) {
        _ => {
            let probe: int = error;
            probe
        },
    }
}

// Facts: `string | baml.errors.Timeout`. The timeout is a fact of the callee
// itself, beside the effect parameter.
function timeout_of_function_value(body: () -> int throws string) -> int {
    baml.future.with_timeout(baml.time.Duration.from_seconds(30), body) catch_all (error) {
        _ => {
            let probe: int = error;
            probe
        },
    }
}

// Facts: `unknown`.
function timeout_of_open_function_value(body: () -> int throws unknown) -> int {
    baml.future.with_timeout(baml.time.Duration.from_seconds(30), body) catch_all (error) {
        _ => {
            let probe: int = error;
            probe
        },
    }
}

// A `catch` that handles the timeout only leaves `string` to the caller, so
// `throws never` is wrong.
function timeout_arm_leaves_the_error_of_the_value(body: () -> int throws string) -> int throws never {
    baml.future.with_timeout(baml.time.Duration.from_seconds(30), body) catch (error) {
        baml.errors.Timeout => 0,
    }
}

// The same call with a lambda written in place had the right facts before.
function timeout_of_lambda() -> int {
    baml.future.with_timeout(baml.time.Duration.from_seconds(30), () -> int throws string {
        throw "boom"
    }) catch_all (error) {
        _ => {
            let probe: int = error;
            probe
        },
    }
}

function pass_through<F>(value: F) -> F {
    value
}

// Facts: `string`. The value reaches `relay` through another generic call, so
// the evidence of the effect parameter sits behind that call's own type
// argument until it is decided.
function relay_of_function_value_behind_a_generic_call(body: () -> int throws string) -> int {
    relay(pass_through(body)) catch_all (error) {
        _ => {
            let probe: int = error;
            probe
        },
    }
}

// A `catch` that handles the timeout only leaves `string` to the caller here
// too.
function timeout_arm_leaves_the_error_of_a_value_behind_a_generic_call(body: () -> int throws string) -> int throws never {
    baml.future.with_timeout(baml.time.Duration.from_seconds(30), pass_through(body)) catch (error) {
        baml.errors.Timeout => 0,
    }
}
