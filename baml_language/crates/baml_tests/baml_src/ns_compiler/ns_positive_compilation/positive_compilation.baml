// Positive compiler cases migrated from Rust compiler2_tir tests. Each fixture
// retains its original isolated source. Successful compilation replaces the
// error-absence checks; warning-sensitive cases also inspect diagnostics.
// Exact inferred types, IR shape, and negative diagnostics remain in Rust.

// From compiler2_tir/array_rest.rs.

test "rest_binding_list_ascription_is_valid" {
    root.compiler.AssertCompiles(
        {
            "test.baml": `
                function f() -> int {
                    let [..let rest: int[]] = [1, 2]
                    return rest[0]
                }
            `,
        },
    )
}

test "rest_binding_union_element_type" {
    root.compiler.AssertCompiles(
        {
            "test.baml": `
                function f(xs: (int | string)[]) -> int {
                    match (xs) {
                        [..let r] => r.length()
                    }
                }
            `,
        },
    )
}

test "rest_binding_evolving_list_scrutinee" {
    root.compiler.AssertCompiles(
        {
            "test.baml": `
                function f() -> int {
                    let xs = [];
                    xs.push(1);
                    match (xs) {
                        [..let r] => r.length()
                    }
                }
            `,
        },
    )
}

test "match_with_rest_binding_arm_is_exhaustive" {
    root.compiler.AssertCompiles(
        {
            "test.baml": `
                function f(xs: int[]) -> int {
                    match (xs) {
                        [] => 0,
                        [let x, ..let r] => x
                    }
                }
            `,
        },
    )
}

// From compiler2_tir/phase8_exceptions.rs.

test "panic_containing_union_after_wildcard_is_not_unreachable" {
    root.compiler.AssertSuccessLacksMessage(
        {
            "test.baml": `
                class AppError {
                  code int
                }

                function fail() -> int {
                  throw AppError { code: 7 }
                }

                function f() -> int {
                  return fail() catch (e) {
                    _ => 1
                    AppError | baml.panics.DivisionByZero => 2
                  }
                }
            `,
        },
        "unreachable arm",
    )
}

test "omitted_inline_lambda_covered_by_declared_throws_is_clean" {
    root.compiler.AssertCompiles(
        {
            "test.baml": `
                function forward(cb: (x: int) -> int) -> int {
                  return cb(1)
                }

                function demo() -> int throws string {
                  return forward((x: int) -> int {
                    throw "boom"
                  })
                }
            `,
        },
    )
}

// The associated Error must satisfy the caller contract and remain visible
// to extraneous-throws analysis.
test "generic_bound_associated_error_is_reused_by_throws_analysis" {
    root.compiler.AssertSuccessLacksMessage(
        {
            "test.baml": `
                class Boom {}

                interface Runner<Input> {
                  type Output
                  type Error

                  function run(self, input: Input) -> Self.Output throws Self.Error
                }

                class Task<T> {
                  function run<Output, Error, R extends Runner<Task<T>, Output = Output, Error = Error>>(
                    self,
                    runner: R,
                  ) -> Output throws Error {
                    runner.run(self)
                  }
                }

                class ConcreteRunner {
                  implements Runner<Task<int>> {
                    type Output = int
                    type Error = Boom

                    function run(self, input: Task<int>) -> int throws Boom {
                      throw Boom {}
                    }
                  }
                }

                function caller(task: Task<int>) -> int throws Boom {
                  task.run(runner = ConcreteRunner {})
                }
            `,
        },
        "extraneous throws declaration",
    )
}

// A break targeting a loop declared inside the defer does not escape it.
test "defer_inner_loop_break_is_allowed" {
    root.compiler.AssertCompiles(
        {
            "test.baml": `
                function f() -> int {
                  defer {
                    for (let x in [1, 2]) {
                      break
                    }
                  }
                  0
                }
            `,
        },
    )
}

// From compiler2_tir/phase3a.rs.

test "optional_param_default_forward_reference_is_scope_aware" {
    root.compiler.AssertCompiles(
        {
            "test.baml": `
                function shadow_later_param(a: int = { let b = 1; b }, b: int = 2) -> int { a }
            `,
        },
    )
}

// Characterize the existing syntactic aliasing-warning boundary: a variable
// containing a mutable value is not an inline literal, even though it aliases.
test "array_filled_with_variable_bound_mutable_value_does_not_warn" {
    root.compiler.AssertSuccessLacksMessage(
        {
            "test.baml": `
                function f() -> int {
                  let x = [0]
                  let rows = baml.Array.filled(3, x)
                  return rows.length()
                }
            `,
        },
        "reuses the same mutable value",
    )
}

// From compiler2_tir/phase7.rs.

test "unannotated_inner_shadow_masks_outer_declared_type" {
    root.compiler.AssertCompiles(
        {
            "test.baml": `
                function f() -> int {
                  let x: int = 1;
                  {
                    let x = "shadow";
                    x = "updated";
                  };
                  return x;
                }
            `,
        },
    )
}

// From compiler2_tir/inference.rs.

test "returning_callback_forwarder_matches_explicit_function_type_return_annotation" {
    root.compiler.AssertCompiles(
        {
            "callback_return.baml": `
                function wrap(cb: (x: int) -> int) -> int {
                  return cb(1)
                }

                function demo() -> ((x: int) -> int throws never) -> int throws never {
                  return (cb: (x: int) -> int throws never) -> int { wrap(cb) }
                }
            `,
        },
    )
}

test "empty_array_local_still_evolves_via_reassignment" {
    root.compiler.AssertCompiles(
        {
            "test.baml": `
                function main() -> int {
                    let a = [];
                    a = [1, 2, 3];
                    return 0;
                }
            `,
        },
    )
}

test "well_typed_index_access_is_accepted" {
    root.compiler.AssertCompiles(
        {
            "test.baml": `
                function main() -> int {
                    let x = [];
                    x[0] = 1;
                    let m = {};
                    m["k"] = 2;
                    return x[0] + m["k"];
                }
            `,
        },
    )
}

// The rejected nullable-index case remains in ns_regression_diagnostics.
test "narrowed_nullable_index_is_accepted" {
    root.compiler.AssertCompiles(
        {
            "test.baml": `
                function main() -> int {
                    let arr = [1, 2, 3];
                    let i: int? = 0;
                    if i != null {
                        return arr[i];
                    }
                    return 0;
                }
            `,
        },
    )
}
