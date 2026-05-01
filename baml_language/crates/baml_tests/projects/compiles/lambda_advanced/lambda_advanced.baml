// ── Chained map: two-step transform ───────────────────────────────
function test_chained_map() -> int[] {
    let items: int[] = [1, 2, 3]
    let step1: int[] = items.map((x) -> { x * 10 })
    step1.map((x) -> { x + 1 })
}

// ── Map over strings to ints ──────────────────────────────────────
function test_map_string_to_int() -> int[] {
    let words: string[] = ["hello", "world"]
    words.map((w: string) -> int { w.length() })
}

// ── Lambda returning a lambda (higher-order return) ───────────────
function test_higher_order_return() -> int {
    let make_adder = (n: int) -> {
        let adder = (x: int) -> int { x + n }
        adder
    }
    let add5 = make_adder(5)
    add5(37)
}

// Lambda calling another lambda
function test_lambda_calling_lambda() -> int {
    let double = (x: int) -> int { x * 2 }
    let apply_twice = (f: (int) -> int, x: int) -> int {
        f(f(x))
    }
    apply_twice(double, 3)
}

// ── Capture across two nesting levels ────────────────────────────
function test_deep_capture() -> int {
    let a = 1
    let f = (b: int) -> {
        let g = (c: int) -> int {
            a + b + c  // captures a from grandparent, b from parent
        }
        g(100)
    }
    f(10) // 1 + 10 + 100 = 111
}

// ── Generic identity used at multiple types ──────────────────────
function test_generic_multi_instantiation() -> int {
    let id = <T>(x: T) -> T { x }
    let a: int = id(42)
    let b: string = id("hello")
    a
}

// ── Lambda with match expression in body ─────────────────────────
function test_lambda_with_match() -> string {
    let describe = (n: int) -> string {
        match (n) {
            0 => "zero"
            1 => "one"
            _ => "many"
        }
    }
    describe(1)
}

// ── Lambda with if/else expression body ──────────────────────────
function test_lambda_with_if() -> string {
    let classify = (n: int) -> string {
        if (n < 0) { "negative" }
        else if (n == 0) { "zero" }
        else { "positive" }
    }
    classify(-5)
}

// ── Map with captured variable in lambda ─────────────────────────
function test_map_with_capture() -> int[] {
    let items: int[] = [1, 2, 3]
    let factor = 10
    items.map((x) -> { x * factor })
}

// ── Nested map: map inside a map callback ────────────────────────
function test_nested_map() -> int[][] {
    let matrix: int[][] = [[1, 2], [3, 4]]
    // Flatten-map: for each row, map and collect
    matrix.map((row: int[]) -> {
        row.map((x) -> { x * x })
    })
}

// ── Map with multi-statement lambda body ─────────────────────────
function test_map_multi_statement() -> int[] {
    let items: int[] = [1, 2, 3]
    items.map((x) -> {
        let doubled = x * 2
        let offset = 100
        doubled + offset
    })
}

// ── Lambda assigned then passed ──────────────────────────────────
function test_lambda_variable_then_pass() -> int[] {
    let items: int[] = [10, 20, 30]
    let halve = (x: int) -> int { x / 2 }
    items.map(halve)
}

// ── Shadowing: lambda param shadows outer variable ───────────────
function test_shadowing() -> string {
    let x = "999"
    let items: int[] = [1, 2, 3]
    let mapped: int[] = items.map((x) -> { x + 1 })
    for (let y in mapped) {
        let i = 0
        while (i < y) {
            x += "1"
            i += 1
        }
    }
    x
}

// ── Immediately-invoked lambda expression ────────────────────────
function test_iife() -> int {
    let result = ((x: int) -> int { x * x })(7) // 49
    result
}

// ── Lambda with optional return type ─────────────────────────────
function test_optional_return() -> int? {
    let maybe = (x: int) -> int? {
        if (x > 0) { x }
        else { null }
    }
    maybe(-1)
}

// ══════════════════════════════════════════════════════════════════
// Advanced type inference scenarios — minimal annotations
// ══════════════════════════════════════════════════════════════════

// ── map with no annotations at all: param AND return inferred ────
function test_fully_inferred_map() -> int[] {
    let items: int[] = [1, 2, 3]
    items.map((x) -> { x * 2 })
    // x: int inferred from T in Array<int>.map
    // return: int inferred from body x * 2
    // U binds to int, result is int[]
}

// ── map where return type changes: infer U != T ──────────────────
function test_map_type_change_inferred() -> string[] {
    let nums: int[] = [1, 2, 3]
    nums.map((n) -> {
        match (n) {
            1 => "one"
            2 => "two"
            _ => "other"
        }
    })
    // n: int inferred, return: string inferred from match body
    // U binds to "one"|"two"|"other", result is string[]?
}

// ── Lambda returning literal 0 — does U bind to 0 or int? ───────
function test_literal_return_inference() -> int[] {
    let items: int[] = [1, 2, 3]
    items.map((x) -> { 0 })
    // body returns literal 0, U binds to 0 (literal type)
    // result is 0[], function returns int[] — needs 0 <: int
}

// ── Compose: fully inferred lambdas ──────────────────────────────
function test_compose_inferred() -> int[] {
    let items: int[] = [1, 2, 3]
    let step1: int[] = items.map((x) -> { x * 2 })
    step1.map((x) -> { x + 1 })
    // Both lambdas: x inferred as int, return inferred as int
}

// ── Higher-order: lambda returning inferred lambda ───────────────
function test_higher_order_inferred() -> int {
    let make_adder = (n: int) -> {
        let adder = (x: int) -> { x + n }
        adder
    }
    // make_adder return type inferred from body: (int) -> int
    let add5 = make_adder(5)
    add5(37)
}

// ── Lambda calling lambda: no return annotations ─────────────────
function test_apply_twice_inferred() -> int {
    let double = (x: int) -> { x * 2 }
    let apply_twice = (f: (int) -> int, x: int) -> { f(f(x)) }
    apply_twice(double, 3)
}

// ── Deep capture with inferred return types ──────────────────────
function test_deep_capture_inferred() -> int {
    let a = 1
    let f = (b: int) -> {
        let g = (c: int) -> { a + b + c }
        g(100)
    }
    f(10)
}

// ── Nested generic: apply with inferred return ───────────────────
function test_generic_apply_inferred() -> int {
    let apply = <T, U>(f: (T) -> U, x: T) -> U { f(x) }
    apply((x) -> { x * 2 }, 21)
    // x: T, but T bound to int from arg 21
    // body x * 2 infers return as int, U binds to int
}

// ── Multiple lambda args: all inferred ───────────────────────────
function test_two_transforms_inferred() -> int {
    let apply_both = (f: (int) -> int, g: (int) -> int, x: int) -> { g(f(x)) }
    apply_both(
        (x: int) -> { x * 2 },
        (x: int) -> { x + 10 },
        5
    )
}

// ── Lambda in match arms: return type unified across arms ────────
function test_lambda_in_match_inferred() -> int {
    let op = "double"
    let f = match (op) {
        "double" => (x: int) -> { x * 2 }
        "negate" => (x: int) -> { 0 - x }
        _ => (x: int) -> { x }
    }
    f(21)
}

// ── Capture + map: inferred capture type flows into lambda ───────
function test_capture_in_map_inferred() -> int[] {
    let items: int[] = [1, 2, 3]
    let factor = 10
    items.map((x) -> { x * factor })
    // x inferred as int, factor captured as int, return int
}

// ── Void lambda: no return value ─────────────────────────────────
function test_void_lambda_inferred() -> int {
    let count = 0
    let inc = () -> { count += 1 }
    inc()
    inc()
    count
}

function test_void_lambda_inferred_captures() -> int {
    let inc = () -> {
        let count = 0
        let inc = () -> { count += 1; count };
        inc
    }();
    inc()
    inc()
    inc()
}


// ── Lambda capturing another lambda ──────────────────────────────
function test_capture_lambda_inferred() -> int {
    let multiply = (a: int, b: int) -> { a * b }
    let double = (x: int) -> { multiply(x, 2) }
    double(21)
}

// ── Map over optional array: param type int? inferred ────────────
function test_map_optional_inferred() -> int[] {
    let items: int?[] = [1, null, 3]
    items.map((x) -> {
        match (x) {
            null => 0
            _ => 1
        }
    })
    // x: int? inferred from T in Array<int?>.map
    // return: 0|1 inferred from match
}

// ── Shadowing with inferred types ────────────────────────────────
function test_shadowing_inferred() -> int[] {
    let x = "not a number"
    let items: int[] = [1, 2, 3]
    items.map((x) -> { x + 1 })
    // lambda x: int (from map context), shadows outer x: string
}

// ── IIFE with inferred types ─────────────────────────────────────
function test_iife_inferred() -> int {
    ((x: int) -> { x * x })(7)
}
