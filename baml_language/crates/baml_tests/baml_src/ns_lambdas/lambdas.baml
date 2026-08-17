// Tests for lambda expressions.
//
// Converted from crates/baml_tests/tests/lambdas.rs. Most bodies are moved into
// top-level helper functions to dodge the VM test-block local boxing bug
// (a value read from a `let` local inside a `test` block compares unequal to a
// literal; locals inside a `function` are not boxed).

// ─── iife_single_param_returns_correct_value ──────────────────────────────────

function lambdas_iife_single_param() -> int {
    let f = (x: int) -> int { x + 1 }
    f(10)
}

test "iife_single_param_returns_correct_value" {
    assert.equal(lambdas_iife_single_param(), 11)
}

// ─── zero_param_lambda_returns_constant ───────────────────────────────────────

function lambdas_zero_param_lambda() -> int {
    let constant = () -> int { 42 }
    constant()
}

test "zero_param_lambda_returns_constant" {
    assert.equal(lambdas_zero_param_lambda(), 42)
}

// ─── multi_param_lambda_returns_sum ───────────────────────────────────────────

function lambdas_multi_param_lambda() -> int {
    let add = (a: int, b: int) -> int { a + b }
    add(20, 22)
}

test "multi_param_lambda_returns_sum" {
    assert.equal(lambdas_multi_param_lambda(), 42)
}

// ─── annotated_lambda_doubles_value ───────────────────────────────────────────

function lambdas_annotated_lambda_doubles() -> int {
    let double = (x: int) -> int { x * 2 }
    double(21)
}

test "annotated_lambda_doubles_value" {
    assert.equal(lambdas_annotated_lambda_doubles(), 42)
}

// ─── inferred_return_type_lambda ──────────────────────────────────────────────

function lambdas_inferred_return_type_lambda() -> int {
    let double = (x: int) -> { x * 2 }
    double(21)
}

test "inferred_return_type_lambda" {
    assert.equal(lambdas_inferred_return_type_lambda(), 42)
}

// ─── lambda_captures_local_variable ───────────────────────────────────────────

function lambdas_captures_local_variable() -> int {
    let base = 10
    let add_base = (x: int) -> int { x + base }
    add_base(5)
}

test "lambda_captures_local_variable" {
    assert.equal(lambdas_captures_local_variable(), 15)
}

// ─── lambda_captures_function_parameter ───────────────────────────────────────

function lambdas_captures_function_parameter() -> int {
    let n = 21
    let double = () -> int { n * 2 }
    double()
}

test "lambda_captures_function_parameter" {
    assert.equal(lambdas_captures_function_parameter(), 42)
}

// ─── lambda_captures_multiple_variables ───────────────────────────────────────

function lambdas_captures_multiple_variables() -> int {
    let a = 10
    let b = 5
    let compute = () -> int { a * b - 8 }
    compute()
}

test "lambda_captures_multiple_variables" {
    assert.equal(lambdas_captures_multiple_variables(), 42)
}

// ─── shared_cell_mutation_counter ─────────────────────────────────────────────

// Returns `count` which is a local cell — moved into a function to avoid boxing.
function lambdas_shared_cell_mutation_counter() -> int {
    let count = 0
    let inc = () -> int { count += 1; count }
    inc()
    inc()
    count
}

test "shared_cell_mutation_counter" {
    assert.equal(lambdas_shared_cell_mutation_counter(), 2)
}

// ─── nested_lambda_transitive_capture ─────────────────────────────────────────

function lambdas_nested_lambda_transitive_capture() -> int {
    let a = 1
    let f = (b: int) -> int {
        let g = (c: int) -> int { a + b + c }
        g(100)
    }
    f(10)
}

test "nested_lambda_transitive_capture" {
    assert.equal(lambdas_nested_lambda_transitive_capture(), 111)
}

// ─── iife_returns_closure_counter ─────────────────────────────────────────────

function lambdas_iife_returns_closure_counter() -> int {
    let inc = () -> {
        let count = 0
        let inner = () -> int { count += 1; count }
        inner
    }()
    inc()
    inc()
    inc()
}

test "iife_returns_closure_counter" {
    assert.equal(lambdas_iife_returns_closure_counter(), 3)
}

// ─── closure_in_map_with_captured_offset ──────────────────────────────────────

function lambdas_closure_in_map_with_captured_offset() -> int[] {
    let offset = 10
    let items: int[] = [1, 2, 3]
    items.map((x: int) -> int { x + offset })
}

test "closure_in_map_with_captured_offset" {
    assert.equal(lambdas_closure_in_map_with_captured_offset(), [11, 12, 13])
}

// ─── multiple_closures_share_cell ─────────────────────────────────────────────

function lambdas_multiple_closures_share_cell() -> int {
    let x = 0
    let inc = () -> int { x += 1; x }
    let dec = () -> int { x -= 1; x }
    inc()
    inc()
    dec()
}

test "multiple_closures_share_cell" {
    assert.equal(lambdas_multiple_closures_share_cell(), 1)
}

// ─── multiple_closures_share_cell_deep_copy ───────────────────────────────────

// Returns `x` which is a local cell — moved into a function to avoid boxing.
function lambdas_multiple_closures_share_cell_deep_copy() -> int {
    let x = 0
    let inc = () -> int { x += 1; x }
    let dec = () -> int { x -= 1; x }
    let inc2 = baml.deep_copy(inc)
    let dec2 = baml.deep_copy(dec)
    inc()
    inc()
    dec2()
    inc2()
    inc2()
    dec2()
    x
}

test "multiple_closures_share_cell_deep_copy" {
    assert.equal(lambdas_multiple_closures_share_cell_deep_copy(), 2)
}

// ─── explicit_throwing_lambda_catches_error ───────────────────────────────────

function lambdas_explicit_throwing_lambda_catches_error() -> int {
    let risky = (x: int) -> int throws string {
        if (x < 0) { throw "negative" }
        x
    }
    risky(-1) catch (e) {
        "negative" => -1,
        _ => -2
    }
}

test "explicit_throwing_lambda_catches_error" {
    assert.equal(lambdas_explicit_throwing_lambda_catches_error(), -1)
}

function lambda_effect_invoke<T, E>(f: () -> T throws E) -> T throws E {
    f()
}

function lambda_effect_forward<E>(f: () -> int throws E) -> int throws E {
    lambda_effect_invoke(() -> { f() })
}

function lambda_effect_fail() -> int throws string {
    throw "callback"
}

function lambda_effect_main() -> int {
    lambda_effect_forward(lambda_effect_fail) catch (e) {
        string => 1
    }
}

test "callback_effect_param_is_inferred_from_unannotated_lambda_body" {
    assert.equal(lambda_effect_main(), 1)
}

function lambda_contextual_call(cb: (x: int) -> int throws int | string) -> int throws int | string {
    cb(1)
}

function lambda_contextual_main() -> int {
    lambda_contextual_call((x: int) -> int { throw "contextual" }) catch (e) {
        string => 1,
        int => 2,
    }
}

test "contextual_lambda_preserves_omitted_throws_surface" {
    assert.equal(lambda_contextual_main(), 1)
}

// ─── lambda_inside_catch_base_keeps_parameter_scope ───────────────────────────

function lambdas_lambda_inside_catch_base_keeps_parameter_scope() -> int {
    {
        let f = (x: int) -> int {
            if (x == 7) { x } else { 0 }
        }
        f(7)
    } catch (x) {
        _ => x
    }
}

test "lambda_inside_catch_base_keeps_parameter_scope" {
    assert.equal(lambdas_lambda_inside_catch_base_keeps_parameter_scope(), 7)
}

// ─── deep_nesting_three_levels ────────────────────────────────────────────────

function lambdas_deep_nesting_three_levels() -> int {
    let a = 1
    let f = (b: int) -> int {
        let g = (c: int) -> int {
            let h = (d: int) -> int { a + b + c + d }
            h(1000)
        }
        g(100)
    }
    f(10)
}

test "deep_nesting_three_levels" {
    assert.equal(lambdas_deep_nesting_three_levels(), 1111)
}

// ─── lambda_captures_loop_variable_accumulation ───────────────────────────────

// Returns `sum` which is a local cell — moved into a function to avoid boxing.
function lambdas_captures_loop_variable_accumulation() -> int {
    let sum = 0
    let items: int[] = [1, 2, 3]
    let add_to_sum = (x: int) -> { sum += x }
    for (let x in items) {
        add_to_sum(x)
    }
    sum
}

test "lambda_captures_loop_variable_accumulation" {
    assert.equal(lambdas_captures_loop_variable_accumulation(), 6)
}

// ─── issue_e_method_resolution_different_types ────────────────────────────────

function lambdas_issue_e_method_resolution_different_types() -> int {
    let arr: int[] = [1, 2, 3]
    let arr_len = arr.length()
    let f = (s: string) -> int { s.length() }
    arr_len * 10 + f("hello")
}

test "issue_e_method_resolution_different_types" {
    assert.equal(lambdas_issue_e_method_resolution_different_types(), 35)
}

// ─── issue_f_shadowing_capture_correct_binding ────────────────────────────────

function lambdas_issue_f_shadowing_capture_correct_binding() -> int {
    let x = 1
    let g = () -> int { x }
    let x = 2
    let f = () -> int { x }
    g() * 10 + f()
}

test "issue_f_shadowing_capture_correct_binding" {
    assert.equal(lambdas_issue_f_shadowing_capture_correct_binding(), 12)
}

// ─── issue_f_shadowing_capture_independent_cells ──────────────────────────────

function lambdas_issue_f_shadowing_capture_independent_cells() -> int {
    let x = 10
    let inc_first = () -> int { x += 1; x }
    let x = 100
    let inc_second = () -> int { x += 1; x }
    inc_first()
    inc_second()
    // first x should be 11, second x should be 101
    // but we can only return one — return inc_first result
    inc_first()
}

test "issue_f_shadowing_capture_independent_cells" {
    assert.equal(lambdas_issue_f_shadowing_capture_independent_cells(), 12)
}

// ─── issue_a_capture_read_after_mutation ──────────────────────────────────────

// `before` and `after` are locals — moved into a function to avoid boxing.
function lambdas_issue_a_capture_read_after_mutation() -> int {
    let x = 0
    let read_x = () -> int { x }
    let inc_x = () -> { x += 1 }
    let before = read_x()
    inc_x()
    let after = read_x()
    before * 10 + after
}

test "issue_a_capture_read_after_mutation" {
    assert.equal(lambdas_issue_a_capture_read_after_mutation(), 1)
}

// ─── issue_a_interleaved_capture_reads_writes ─────────────────────────────────

// `a`, `b`, `c` are locals — moved into a function to avoid boxing.
function lambdas_issue_a_interleaved_capture_reads_writes() -> int {
    let x = 0
    let set_x = (v: int) -> { x = v }
    let get_x = () -> int { x }
    set_x(10)
    let a = get_x()
    set_x(20)
    let b = get_x()
    set_x(30)
    let c = get_x()
    a + b + c
}

test "issue_a_interleaved_capture_reads_writes" {
    assert.equal(lambdas_issue_a_interleaved_capture_reads_writes(), 60)
}

// ─── issue_d_capture_as_call_destination ─────────────────────────────────────

function lambdas_helper_42() -> int { 42 }

function lambdas_issue_d_capture_as_call_destination() -> int {
    let x = 0
    let f = () -> int {
        x = lambdas_helper_42()
        x
    }
    f()
}

test "issue_d_capture_as_call_destination" {
    assert.equal(lambdas_issue_d_capture_as_call_destination(), 42)
}

// ─── issue_b_captured_variable_mutation_visible ───────────────────────────────

// Returns `x` (a cell local) — moved into a function.
function lambdas_issue_b_captured_variable_mutation_visible() -> int {
    let x = 0
    let set = (v: int) -> { x = v }
    set(42)
    x
}

test "issue_b_captured_variable_mutation_visible" {
    assert.equal(lambdas_issue_b_captured_variable_mutation_visible(), 42)
}


// ─── lambda_param_shadows_annotated_outer_local ───────────────────────────────

function lambdas_lambda_param_shadows_annotated_outer_local() -> int {
    let x: int = 7;
    let f = (x: string) -> int {
        x = "world";
        x.length()
    };
    f("hi") + x
}

test "lambda_param_shadows_annotated_outer_local" {
    assert.equal(lambdas_lambda_param_shadows_annotated_outer_local(), 12)
}


// ─── inferred_signature_is_reconstructed ─────────────────────────────────────

// A lambda's runtime signature carries the types it actually HAS, not only the
// ones it spells. An omitted return type and an omitted `throws` clause are
// filled from inference, so a closure value reconstructs as precisely as an
// annotated declaration would — `-> int throws never` here, not the
// `-> unknown throws unknown` its syntax alone says.
function lambdas_inferred_signature() -> string {
    let inferred = (x: int) -> { x * 2 }
    let sig = reflect.signature(inferred)
    sig.returns.to_string() + "/" + sig.errors.to_string()
}

test "inferred_signature_is_reconstructed" {
    assert.equal(lambdas_inferred_signature(), "int/never")
}

// The same for a throwing body: the inferred `throws` surface is the error set
// the body can actually raise, so it is testable rather than an open claim.
function lambdas_inferred_throwing_signature() -> string {
    let inferred = (x: int) -> {
        if (x < 0) { throw "negative" }
        x * 2
    }
    let sig = reflect.signature(inferred)
    sig.returns.to_string() + "/" + sig.errors.to_string()
}

test "inferred_throwing_signature_is_reconstructed" {
    assert.equal(lambdas_inferred_throwing_signature(), "int/\"negative\"")
}
