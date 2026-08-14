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
    let apply_twice = (f: (int) -> int throws never, x: int) -> int {
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
    let apply_twice = (f: (int) -> int throws never, x: int) -> { f(f(x)) }
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

// ── Multiple lambda args: all inferred ───────────────────────────
function test_two_transforms_inferred() -> int {
    let apply_both = (f: (int) -> int throws never, g: (int) -> int throws never, x: int) -> { g(f(x)) }
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

//----
//- diagnostics
// <no-diagnostics-expected>
//
//- semantic_tokens
// lambda_advanced.baml:1:1 (comment) len=135 "// ── Chained map: two-step transform ───────────────────────────────"
// lambda_advanced.baml:2:1 (keyword) len=8 "function"
// lambda_advanced.baml:2:10 (function) [declaration] len=16 "test_chained_map"
// lambda_advanced.baml:2:32 (type) [defaultLibrary] len=3 "int"
// lambda_advanced.baml:3:5 (keyword) len=3 "let"
// lambda_advanced.baml:3:9 (variable) [declaration] len=5 "items"
// lambda_advanced.baml:3:16 (type) [defaultLibrary] len=3 "int"
// lambda_advanced.baml:3:22 (operator) len=1 "="
// lambda_advanced.baml:3:25 (number) len=1 "1"
// lambda_advanced.baml:3:28 (number) len=1 "2"
// lambda_advanced.baml:3:31 (number) len=1 "3"
// lambda_advanced.baml:4:5 (keyword) len=3 "let"
// lambda_advanced.baml:4:9 (variable) [declaration] len=5 "step1"
// lambda_advanced.baml:4:16 (type) [defaultLibrary] len=3 "int"
// lambda_advanced.baml:4:22 (operator) len=1 "="
// lambda_advanced.baml:4:24 (variable) len=5 "items"
// lambda_advanced.baml:4:30 (method) len=3 "map"
// lambda_advanced.baml:4:35 (parameter) [declaration] len=1 "x"
// lambda_advanced.baml:4:43 (parameter) len=1 "x"
// lambda_advanced.baml:4:45 (operator) len=1 "*"
// lambda_advanced.baml:4:47 (number) len=2 "10"
// lambda_advanced.baml:5:5 (variable) len=5 "step1"
// lambda_advanced.baml:5:11 (method) len=3 "map"
// lambda_advanced.baml:5:16 (parameter) [declaration] len=1 "x"
// lambda_advanced.baml:5:24 (parameter) len=1 "x"
// lambda_advanced.baml:5:26 (operator) len=1 "+"
// lambda_advanced.baml:5:28 (number) len=1 "1"
// lambda_advanced.baml:8:1 (comment) len=149 "// ── Map over strings to ints ──────────────────────────────────────"
// lambda_advanced.baml:9:1 (keyword) len=8 "function"
// lambda_advanced.baml:9:10 (function) [declaration] len=22 "test_map_string_to_int"
// lambda_advanced.baml:9:38 (type) [defaultLibrary] len=3 "int"
// lambda_advanced.baml:10:5 (keyword) len=3 "let"
// lambda_advanced.baml:10:9 (variable) [declaration] len=5 "words"
// lambda_advanced.baml:10:16 (type) [defaultLibrary] len=6 "string"
// lambda_advanced.baml:10:25 (operator) len=1 "="
// lambda_advanced.baml:10:28 (string) len=7 "\"hello\""
// lambda_advanced.baml:10:37 (string) len=7 "\"world\""
// lambda_advanced.baml:11:5 (variable) len=5 "words"
// lambda_advanced.baml:11:11 (method) len=3 "map"
// lambda_advanced.baml:11:16 (parameter) [declaration] len=1 "w"
// lambda_advanced.baml:11:19 (type) [defaultLibrary] len=6 "string"
// lambda_advanced.baml:11:30 (type) [defaultLibrary] len=3 "int"
// lambda_advanced.baml:11:36 (parameter) len=1 "w"
// lambda_advanced.baml:11:38 (method) len=6 "length"
// lambda_advanced.baml:14:1 (comment) len=103 "// ── Lambda returning a lambda (higher-order return) ───────────────"
// lambda_advanced.baml:15:1 (keyword) len=8 "function"
// lambda_advanced.baml:15:10 (function) [declaration] len=24 "test_higher_order_return"
// lambda_advanced.baml:15:40 (type) [defaultLibrary] len=3 "int"
// lambda_advanced.baml:16:5 (keyword) len=3 "let"
// lambda_advanced.baml:16:9 (variable) [declaration] len=10 "make_adder"
// lambda_advanced.baml:16:20 (operator) len=1 "="
// lambda_advanced.baml:16:23 (parameter) [declaration] len=1 "n"
// lambda_advanced.baml:16:26 (type) [defaultLibrary] len=3 "int"
// lambda_advanced.baml:17:9 (keyword) len=3 "let"
// lambda_advanced.baml:17:13 (variable) [declaration] len=5 "adder"
// lambda_advanced.baml:17:19 (operator) len=1 "="
// lambda_advanced.baml:17:22 (parameter) [declaration] len=1 "x"
// lambda_advanced.baml:17:25 (type) [defaultLibrary] len=3 "int"
// lambda_advanced.baml:17:33 (type) [defaultLibrary] len=3 "int"
// lambda_advanced.baml:17:39 (parameter) len=1 "x"
// lambda_advanced.baml:17:41 (operator) len=1 "+"
// lambda_advanced.baml:17:43 (parameter) len=1 "n"
// lambda_advanced.baml:18:9 (variable) len=5 "adder"
// lambda_advanced.baml:20:5 (keyword) len=3 "let"
// lambda_advanced.baml:20:9 (variable) [declaration] len=4 "add5"
// lambda_advanced.baml:20:14 (operator) len=1 "="
// lambda_advanced.baml:20:16 (variable) len=10 "make_adder"
// lambda_advanced.baml:20:27 (number) len=1 "5"
// lambda_advanced.baml:21:5 (variable) len=4 "add5"
// lambda_advanced.baml:21:10 (number) len=2 "37"
// lambda_advanced.baml:24:1 (comment) len=32 "// Lambda calling another lambda"
// lambda_advanced.baml:25:1 (keyword) len=8 "function"
// lambda_advanced.baml:25:10 (function) [declaration] len=26 "test_lambda_calling_lambda"
// lambda_advanced.baml:25:42 (type) [defaultLibrary] len=3 "int"
// lambda_advanced.baml:26:5 (keyword) len=3 "let"
// lambda_advanced.baml:26:9 (variable) [declaration] len=6 "double"
// lambda_advanced.baml:26:16 (operator) len=1 "="
// lambda_advanced.baml:26:19 (parameter) [declaration] len=1 "x"
// lambda_advanced.baml:26:22 (type) [defaultLibrary] len=3 "int"
// lambda_advanced.baml:26:30 (type) [defaultLibrary] len=3 "int"
// lambda_advanced.baml:26:36 (parameter) len=1 "x"
// lambda_advanced.baml:26:38 (operator) len=1 "*"
// lambda_advanced.baml:26:40 (number) len=1 "2"
// lambda_advanced.baml:27:5 (keyword) len=3 "let"
// lambda_advanced.baml:27:9 (variable) [declaration] len=11 "apply_twice"
// lambda_advanced.baml:27:21 (operator) len=1 "="
// lambda_advanced.baml:27:24 (parameter) [declaration] len=1 "f"
// lambda_advanced.baml:27:28 (type) [defaultLibrary] len=3 "int"
// lambda_advanced.baml:27:36 (type) [defaultLibrary] len=3 "int"
// lambda_advanced.baml:27:40 (keyword) len=6 "throws"
// lambda_advanced.baml:27:47 (type) [defaultLibrary] len=5 "never"
// lambda_advanced.baml:27:54 (parameter) [declaration] len=1 "x"
// lambda_advanced.baml:27:57 (type) [defaultLibrary] len=3 "int"
// lambda_advanced.baml:27:65 (type) [defaultLibrary] len=3 "int"
// lambda_advanced.baml:28:9 (parameter) len=1 "f"
// lambda_advanced.baml:28:11 (parameter) len=1 "f"
// lambda_advanced.baml:28:13 (parameter) len=1 "x"
// lambda_advanced.baml:30:5 (variable) len=11 "apply_twice"
// lambda_advanced.baml:30:17 (variable) len=6 "double"
// lambda_advanced.baml:30:25 (number) len=1 "3"
// lambda_advanced.baml:33:1 (comment) len=128 "// ── Capture across two nesting levels ────────────────────────────"
// lambda_advanced.baml:34:1 (keyword) len=8 "function"
// lambda_advanced.baml:34:10 (function) [declaration] len=17 "test_deep_capture"
// lambda_advanced.baml:34:33 (type) [defaultLibrary] len=3 "int"
// lambda_advanced.baml:35:5 (keyword) len=3 "let"
// lambda_advanced.baml:35:9 (variable) [declaration] len=1 "a"
// lambda_advanced.baml:35:11 (operator) len=1 "="
// lambda_advanced.baml:35:13 (number) len=1 "1"
// lambda_advanced.baml:36:5 (keyword) len=3 "let"
// lambda_advanced.baml:36:9 (variable) [declaration] len=1 "f"
// lambda_advanced.baml:36:11 (operator) len=1 "="
// lambda_advanced.baml:36:14 (parameter) [declaration] len=1 "b"
// lambda_advanced.baml:36:17 (type) [defaultLibrary] len=3 "int"
// lambda_advanced.baml:37:9 (keyword) len=3 "let"
// lambda_advanced.baml:37:13 (variable) [declaration] len=1 "g"
// lambda_advanced.baml:37:15 (operator) len=1 "="
// lambda_advanced.baml:37:18 (parameter) [declaration] len=1 "c"
// lambda_advanced.baml:37:21 (type) [defaultLibrary] len=3 "int"
// lambda_advanced.baml:37:29 (type) [defaultLibrary] len=3 "int"
// lambda_advanced.baml:38:13 (variable) len=1 "a"
// lambda_advanced.baml:38:15 (operator) len=1 "+"
// lambda_advanced.baml:38:17 (parameter) len=1 "b"
// lambda_advanced.baml:38:19 (operator) len=1 "+"
// lambda_advanced.baml:38:21 (parameter) len=1 "c"
// lambda_advanced.baml:38:24 (comment) len=45 "// captures a from grandparent, b from parent"
// lambda_advanced.baml:40:9 (variable) len=1 "g"
// lambda_advanced.baml:40:11 (number) len=3 "100"
// lambda_advanced.baml:42:5 (variable) len=1 "f"
// lambda_advanced.baml:42:7 (number) len=2 "10"
// lambda_advanced.baml:42:11 (comment) len=21 "// 1 + 10 + 100 = 111"
// lambda_advanced.baml:45:1 (comment) len=122 "// ── Lambda with match expression in body ─────────────────────────"
// lambda_advanced.baml:46:1 (keyword) len=8 "function"
// lambda_advanced.baml:46:10 (function) [declaration] len=22 "test_lambda_with_match"
// lambda_advanced.baml:46:38 (type) [defaultLibrary] len=6 "string"
// lambda_advanced.baml:47:5 (keyword) len=3 "let"
// lambda_advanced.baml:47:9 (variable) [declaration] len=8 "describe"
// lambda_advanced.baml:47:18 (operator) len=1 "="
// lambda_advanced.baml:47:21 (parameter) [declaration] len=1 "n"
// lambda_advanced.baml:47:24 (type) [defaultLibrary] len=3 "int"
// lambda_advanced.baml:47:32 (type) [defaultLibrary] len=6 "string"
// lambda_advanced.baml:48:9 (keyword) len=5 "match"
// lambda_advanced.baml:48:16 (parameter) len=1 "n"
// lambda_advanced.baml:49:13 (number) len=1 "0"
// lambda_advanced.baml:49:18 (string) len=6 "\"zero\""
// lambda_advanced.baml:50:13 (number) len=1 "1"
// lambda_advanced.baml:50:18 (string) len=5 "\"one\""
// lambda_advanced.baml:51:18 (string) len=6 "\"many\""
// lambda_advanced.baml:54:5 (variable) len=8 "describe"
// lambda_advanced.baml:54:14 (number) len=1 "1"
// lambda_advanced.baml:57:1 (comment) len=124 "// ── Lambda with if/else expression body ──────────────────────────"
// lambda_advanced.baml:58:1 (keyword) len=8 "function"
// lambda_advanced.baml:58:10 (function) [declaration] len=19 "test_lambda_with_if"
// lambda_advanced.baml:58:35 (type) [defaultLibrary] len=6 "string"
// lambda_advanced.baml:59:5 (keyword) len=3 "let"
// lambda_advanced.baml:59:9 (variable) [declaration] len=8 "classify"
// lambda_advanced.baml:59:18 (operator) len=1 "="
// lambda_advanced.baml:59:21 (parameter) [declaration] len=1 "n"
// lambda_advanced.baml:59:24 (type) [defaultLibrary] len=3 "int"
// lambda_advanced.baml:59:32 (type) [defaultLibrary] len=6 "string"
// lambda_advanced.baml:60:9 (keyword) len=2 "if"
// lambda_advanced.baml:60:13 (parameter) len=1 "n"
// lambda_advanced.baml:60:15 (operator) len=1 "<"
// lambda_advanced.baml:60:17 (number) len=1 "0"
// lambda_advanced.baml:60:22 (string) len=10 "\"negative\""
// lambda_advanced.baml:61:9 (keyword) len=4 "else"
// lambda_advanced.baml:61:14 (keyword) len=2 "if"
// lambda_advanced.baml:61:18 (parameter) len=1 "n"
// lambda_advanced.baml:61:20 (operator) len=2 "=="
// lambda_advanced.baml:61:23 (number) len=1 "0"
// lambda_advanced.baml:61:28 (string) len=6 "\"zero\""
// lambda_advanced.baml:62:9 (keyword) len=4 "else"
// lambda_advanced.baml:62:16 (string) len=10 "\"positive\""
// lambda_advanced.baml:64:5 (variable) len=8 "classify"
// lambda_advanced.baml:64:14 (operator) len=1 "-"
// lambda_advanced.baml:64:15 (number) len=1 "5"
// lambda_advanced.baml:67:1 (comment) len=122 "// ── Map with captured variable in lambda ─────────────────────────"
// lambda_advanced.baml:68:1 (keyword) len=8 "function"
// lambda_advanced.baml:68:10 (function) [declaration] len=21 "test_map_with_capture"
// lambda_advanced.baml:68:37 (type) [defaultLibrary] len=3 "int"
// lambda_advanced.baml:69:5 (keyword) len=3 "let"
// lambda_advanced.baml:69:9 (variable) [declaration] len=5 "items"
// lambda_advanced.baml:69:16 (type) [defaultLibrary] len=3 "int"
// lambda_advanced.baml:69:22 (operator) len=1 "="
// lambda_advanced.baml:69:25 (number) len=1 "1"
// lambda_advanced.baml:69:28 (number) len=1 "2"
// lambda_advanced.baml:69:31 (number) len=1 "3"
// lambda_advanced.baml:70:5 (keyword) len=3 "let"
// lambda_advanced.baml:70:9 (variable) [declaration] len=6 "factor"
// lambda_advanced.baml:70:16 (operator) len=1 "="
// lambda_advanced.baml:70:18 (number) len=2 "10"
// lambda_advanced.baml:71:5 (variable) len=5 "items"
// lambda_advanced.baml:71:11 (method) len=3 "map"
// lambda_advanced.baml:71:16 (parameter) [declaration] len=1 "x"
// lambda_advanced.baml:71:24 (parameter) len=1 "x"
// lambda_advanced.baml:71:26 (operator) len=1 "*"
// lambda_advanced.baml:71:28 (variable) len=6 "factor"
// lambda_advanced.baml:74:1 (comment) len=120 "// ── Nested map: map inside a map callback ────────────────────────"
// lambda_advanced.baml:75:1 (keyword) len=8 "function"
// lambda_advanced.baml:75:10 (function) [declaration] len=15 "test_nested_map"
// lambda_advanced.baml:75:31 (type) [defaultLibrary] len=3 "int"
// lambda_advanced.baml:76:5 (keyword) len=3 "let"
// lambda_advanced.baml:76:9 (variable) [declaration] len=6 "matrix"
// lambda_advanced.baml:76:17 (type) [defaultLibrary] len=3 "int"
// lambda_advanced.baml:76:25 (operator) len=1 "="
// lambda_advanced.baml:76:29 (number) len=1 "1"
// lambda_advanced.baml:76:32 (number) len=1 "2"
// lambda_advanced.baml:76:37 (number) len=1 "3"
// lambda_advanced.baml:76:40 (number) len=1 "4"
// lambda_advanced.baml:77:5 (comment) len=45 "// Flatten-map: for each row, map and collect"
// lambda_advanced.baml:78:5 (variable) len=6 "matrix"
// lambda_advanced.baml:78:12 (method) len=3 "map"
// lambda_advanced.baml:78:17 (parameter) [declaration] len=3 "row"
// lambda_advanced.baml:78:22 (type) [defaultLibrary] len=3 "int"
// lambda_advanced.baml:79:9 (parameter) len=3 "row"
// lambda_advanced.baml:79:13 (method) len=3 "map"
// lambda_advanced.baml:79:18 (parameter) [declaration] len=1 "x"
// lambda_advanced.baml:79:26 (parameter) len=1 "x"
// lambda_advanced.baml:79:28 (operator) len=1 "*"
// lambda_advanced.baml:79:30 (parameter) len=1 "x"
// lambda_advanced.baml:83:1 (comment) len=122 "// ── Map with multi-statement lambda body ─────────────────────────"
// lambda_advanced.baml:84:1 (keyword) len=8 "function"
// lambda_advanced.baml:84:10 (function) [declaration] len=24 "test_map_multi_statement"
// lambda_advanced.baml:84:40 (type) [defaultLibrary] len=3 "int"
// lambda_advanced.baml:85:5 (keyword) len=3 "let"
// lambda_advanced.baml:85:9 (variable) [declaration] len=5 "items"
// lambda_advanced.baml:85:16 (type) [defaultLibrary] len=3 "int"
// lambda_advanced.baml:85:22 (operator) len=1 "="
// lambda_advanced.baml:85:25 (number) len=1 "1"
// lambda_advanced.baml:85:28 (number) len=1 "2"
// lambda_advanced.baml:85:31 (number) len=1 "3"
// lambda_advanced.baml:86:5 (variable) len=5 "items"
// lambda_advanced.baml:86:11 (method) len=3 "map"
// lambda_advanced.baml:86:16 (parameter) [declaration] len=1 "x"
// lambda_advanced.baml:87:9 (keyword) len=3 "let"
// lambda_advanced.baml:87:13 (variable) [declaration] len=7 "doubled"
// lambda_advanced.baml:87:21 (operator) len=1 "="
// lambda_advanced.baml:87:23 (parameter) len=1 "x"
// lambda_advanced.baml:87:25 (operator) len=1 "*"
// lambda_advanced.baml:87:27 (number) len=1 "2"
// lambda_advanced.baml:88:9 (keyword) len=3 "let"
// lambda_advanced.baml:88:13 (variable) [declaration] len=6 "offset"
// lambda_advanced.baml:88:20 (operator) len=1 "="
// lambda_advanced.baml:88:22 (number) len=3 "100"
// lambda_advanced.baml:89:9 (variable) len=7 "doubled"
// lambda_advanced.baml:89:17 (operator) len=1 "+"
// lambda_advanced.baml:89:19 (variable) len=6 "offset"
// lambda_advanced.baml:93:1 (comment) len=140 "// ── Lambda assigned then passed ──────────────────────────────────"
// lambda_advanced.baml:94:1 (keyword) len=8 "function"
// lambda_advanced.baml:94:10 (function) [declaration] len=30 "test_lambda_variable_then_pass"
// lambda_advanced.baml:94:46 (type) [defaultLibrary] len=3 "int"
// lambda_advanced.baml:95:5 (keyword) len=3 "let"
// lambda_advanced.baml:95:9 (variable) [declaration] len=5 "items"
// lambda_advanced.baml:95:16 (type) [defaultLibrary] len=3 "int"
// lambda_advanced.baml:95:22 (operator) len=1 "="
// lambda_advanced.baml:95:25 (number) len=2 "10"
// lambda_advanced.baml:95:29 (number) len=2 "20"
// lambda_advanced.baml:95:33 (number) len=2 "30"
// lambda_advanced.baml:96:5 (keyword) len=3 "let"
// lambda_advanced.baml:96:9 (variable) [declaration] len=5 "halve"
// lambda_advanced.baml:96:15 (operator) len=1 "="
// lambda_advanced.baml:96:18 (parameter) [declaration] len=1 "x"
// lambda_advanced.baml:96:21 (type) [defaultLibrary] len=3 "int"
// lambda_advanced.baml:96:29 (type) [defaultLibrary] len=3 "int"
// lambda_advanced.baml:96:35 (parameter) len=1 "x"
// lambda_advanced.baml:96:37 (operator) len=1 "/"
// lambda_advanced.baml:96:39 (number) len=1 "2"
// lambda_advanced.baml:97:5 (variable) len=5 "items"
// lambda_advanced.baml:97:11 (method) len=3 "map"
// lambda_advanced.baml:97:15 (variable) len=5 "halve"
// lambda_advanced.baml:100:1 (comment) len=102 "// ── Shadowing: lambda param shadows outer variable ───────────────"
// lambda_advanced.baml:101:1 (keyword) len=8 "function"
// lambda_advanced.baml:101:10 (function) [declaration] len=14 "test_shadowing"
// lambda_advanced.baml:101:30 (type) [defaultLibrary] len=6 "string"
// lambda_advanced.baml:102:5 (keyword) len=3 "let"
// lambda_advanced.baml:102:9 (variable) [declaration] len=1 "x"
// lambda_advanced.baml:102:11 (operator) len=1 "="
// lambda_advanced.baml:102:13 (string) len=5 "\"999\""
// lambda_advanced.baml:103:5 (keyword) len=3 "let"
// lambda_advanced.baml:103:9 (variable) [declaration] len=5 "items"
// lambda_advanced.baml:103:16 (type) [defaultLibrary] len=3 "int"
// lambda_advanced.baml:103:22 (operator) len=1 "="
// lambda_advanced.baml:103:25 (number) len=1 "1"
// lambda_advanced.baml:103:28 (number) len=1 "2"
// lambda_advanced.baml:103:31 (number) len=1 "3"
// lambda_advanced.baml:104:5 (keyword) len=3 "let"
// lambda_advanced.baml:104:9 (variable) [declaration] len=6 "mapped"
// lambda_advanced.baml:104:17 (type) [defaultLibrary] len=3 "int"
// lambda_advanced.baml:104:23 (operator) len=1 "="
// lambda_advanced.baml:104:25 (variable) len=5 "items"
// lambda_advanced.baml:104:31 (method) len=3 "map"
// lambda_advanced.baml:104:36 (parameter) [declaration] len=1 "x"
// lambda_advanced.baml:104:44 (parameter) len=1 "x"
// lambda_advanced.baml:104:46 (operator) len=1 "+"
// lambda_advanced.baml:104:48 (number) len=1 "1"
// lambda_advanced.baml:105:5 (keyword) len=3 "for"
// lambda_advanced.baml:105:10 (keyword) len=3 "let"
// lambda_advanced.baml:105:14 (variable) [declaration] len=1 "y"
// lambda_advanced.baml:105:16 (keyword) len=2 "in"
// lambda_advanced.baml:105:19 (variable) len=6 "mapped"
// lambda_advanced.baml:106:9 (keyword) len=3 "let"
// lambda_advanced.baml:106:13 (variable) [declaration] len=1 "i"
// lambda_advanced.baml:106:15 (operator) len=1 "="
// lambda_advanced.baml:106:17 (number) len=1 "0"
// lambda_advanced.baml:107:9 (keyword) len=5 "while"
// lambda_advanced.baml:107:16 (variable) len=1 "i"
// lambda_advanced.baml:107:18 (operator) len=1 "<"
// lambda_advanced.baml:107:20 (variable) len=1 "y"
// lambda_advanced.baml:108:13 (variable) len=1 "x"
// lambda_advanced.baml:108:15 (operator) len=2 "+="
// lambda_advanced.baml:108:18 (string) len=3 "\"1\""
// lambda_advanced.baml:109:13 (variable) len=1 "i"
// lambda_advanced.baml:109:15 (operator) len=2 "+="
// lambda_advanced.baml:109:18 (number) len=1 "1"
// lambda_advanced.baml:112:5 (variable) len=1 "x"
// lambda_advanced.baml:115:1 (comment) len=120 "// ── Immediately-invoked lambda expression ────────────────────────"
// lambda_advanced.baml:116:1 (keyword) len=8 "function"
// lambda_advanced.baml:116:10 (function) [declaration] len=9 "test_iife"
// lambda_advanced.baml:116:25 (type) [defaultLibrary] len=3 "int"
// lambda_advanced.baml:117:5 (keyword) len=3 "let"
// lambda_advanced.baml:117:9 (variable) [declaration] len=6 "result"
// lambda_advanced.baml:117:16 (operator) len=1 "="
// lambda_advanced.baml:117:20 (parameter) [declaration] len=1 "x"
// lambda_advanced.baml:117:23 (type) [defaultLibrary] len=3 "int"
// lambda_advanced.baml:117:31 (type) [defaultLibrary] len=3 "int"
// lambda_advanced.baml:117:37 (parameter) len=1 "x"
// lambda_advanced.baml:117:39 (operator) len=1 "*"
// lambda_advanced.baml:117:41 (parameter) len=1 "x"
// lambda_advanced.baml:117:46 (number) len=1 "7"
// lambda_advanced.baml:117:49 (comment) len=5 "// 49"
// lambda_advanced.baml:118:5 (variable) len=6 "result"
// lambda_advanced.baml:121:1 (comment) len=130 "// ── Lambda with optional return type ─────────────────────────────"
// lambda_advanced.baml:122:1 (keyword) len=8 "function"
// lambda_advanced.baml:122:10 (function) [declaration] len=20 "test_optional_return"
// lambda_advanced.baml:122:36 (type) [defaultLibrary] len=3 "int"
// lambda_advanced.baml:123:5 (keyword) len=3 "let"
// lambda_advanced.baml:123:9 (variable) [declaration] len=5 "maybe"
// lambda_advanced.baml:123:15 (operator) len=1 "="
// lambda_advanced.baml:123:18 (parameter) [declaration] len=1 "x"
// lambda_advanced.baml:123:21 (type) [defaultLibrary] len=3 "int"
// lambda_advanced.baml:123:29 (type) [defaultLibrary] len=3 "int"
// lambda_advanced.baml:124:9 (keyword) len=2 "if"
// lambda_advanced.baml:124:13 (parameter) len=1 "x"
// lambda_advanced.baml:124:15 (operator) len=1 ">"
// lambda_advanced.baml:124:17 (number) len=1 "0"
// lambda_advanced.baml:124:22 (parameter) len=1 "x"
// lambda_advanced.baml:125:9 (keyword) len=4 "else"
// lambda_advanced.baml:125:16 (type) [defaultLibrary] len=4 "null"
// lambda_advanced.baml:127:5 (variable) len=5 "maybe"
// lambda_advanced.baml:127:11 (operator) len=1 "-"
// lambda_advanced.baml:127:12 (number) len=1 "1"
// lambda_advanced.baml:130:1 (comment) len=201 "// ══════════════════════════════════════════════════════════════════"
// lambda_advanced.baml:131:1 (comment) len=60 "// Advanced type inference scenarios — minimal annotations"
// lambda_advanced.baml:132:1 (comment) len=201 "// ══════════════════════════════════════════════════════════════════"
// lambda_advanced.baml:134:1 (comment) len=80 "// ── map with no annotations at all: param AND return inferred ────"
// lambda_advanced.baml:135:1 (keyword) len=8 "function"
// lambda_advanced.baml:135:10 (function) [declaration] len=23 "test_fully_inferred_map"
// lambda_advanced.baml:135:39 (type) [defaultLibrary] len=3 "int"
// lambda_advanced.baml:136:5 (keyword) len=3 "let"
// lambda_advanced.baml:136:9 (variable) [declaration] len=5 "items"
// lambda_advanced.baml:136:16 (type) [defaultLibrary] len=3 "int"
// lambda_advanced.baml:136:22 (operator) len=1 "="
// lambda_advanced.baml:136:25 (number) len=1 "1"
// lambda_advanced.baml:136:28 (number) len=1 "2"
// lambda_advanced.baml:136:31 (number) len=1 "3"
// lambda_advanced.baml:137:5 (variable) len=5 "items"
// lambda_advanced.baml:137:11 (method) len=3 "map"
// lambda_advanced.baml:137:16 (parameter) [declaration] len=1 "x"
// lambda_advanced.baml:137:24 (parameter) len=1 "x"
// lambda_advanced.baml:137:26 (operator) len=1 "*"
// lambda_advanced.baml:137:28 (number) len=1 "2"
// lambda_advanced.baml:138:5 (comment) len=43 "// x: int inferred from T in Array<int>.map"
// lambda_advanced.baml:139:5 (comment) len=39 "// return: int inferred from body x * 2"
// lambda_advanced.baml:140:5 (comment) len=34 "// U binds to int, result is int[]"
// lambda_advanced.baml:143:1 (comment) len=108 "// ── map where return type changes: infer U != T ──────────────────"
// lambda_advanced.baml:144:1 (keyword) len=8 "function"
// lambda_advanced.baml:144:10 (function) [declaration] len=29 "test_map_type_change_inferred"
// lambda_advanced.baml:144:45 (type) [defaultLibrary] len=6 "string"
// lambda_advanced.baml:145:5 (keyword) len=3 "let"
// lambda_advanced.baml:145:9 (variable) [declaration] len=4 "nums"
// lambda_advanced.baml:145:15 (type) [defaultLibrary] len=3 "int"
// lambda_advanced.baml:145:21 (operator) len=1 "="
// lambda_advanced.baml:145:24 (number) len=1 "1"
// lambda_advanced.baml:145:27 (number) len=1 "2"
// lambda_advanced.baml:145:30 (number) len=1 "3"
// lambda_advanced.baml:146:5 (variable) len=4 "nums"
// lambda_advanced.baml:146:10 (method) len=3 "map"
// lambda_advanced.baml:146:15 (parameter) [declaration] len=1 "n"
// lambda_advanced.baml:147:9 (keyword) len=5 "match"
// lambda_advanced.baml:147:16 (parameter) len=1 "n"
// lambda_advanced.baml:148:13 (number) len=1 "1"
// lambda_advanced.baml:148:18 (string) len=5 "\"one\""
// lambda_advanced.baml:149:13 (number) len=1 "2"
// lambda_advanced.baml:149:18 (string) len=5 "\"two\""
// lambda_advanced.baml:150:18 (string) len=7 "\"other\""
// lambda_advanced.baml:153:5 (comment) len=59 "// n: int inferred, return: string inferred from match body"
// lambda_advanced.baml:154:5 (comment) len=54 "// U binds to \"one\"|\"two\"|\"other\", result is string[]?"
// lambda_advanced.baml:157:1 (comment) len=87 "// ── Lambda returning literal 0 — does U bind to 0 or int? ───────"
// lambda_advanced.baml:158:1 (keyword) len=8 "function"
// lambda_advanced.baml:158:10 (function) [declaration] len=29 "test_literal_return_inference"
// lambda_advanced.baml:158:45 (type) [defaultLibrary] len=3 "int"
// lambda_advanced.baml:159:5 (keyword) len=3 "let"
// lambda_advanced.baml:159:9 (variable) [declaration] len=5 "items"
// lambda_advanced.baml:159:16 (type) [defaultLibrary] len=3 "int"
// lambda_advanced.baml:159:22 (operator) len=1 "="
// lambda_advanced.baml:159:25 (number) len=1 "1"
// lambda_advanced.baml:159:28 (number) len=1 "2"
// lambda_advanced.baml:159:31 (number) len=1 "3"
// lambda_advanced.baml:160:5 (variable) len=5 "items"
// lambda_advanced.baml:160:11 (method) len=3 "map"
// lambda_advanced.baml:160:16 (parameter) [declaration] len=1 "x"
// lambda_advanced.baml:160:24 (number) len=1 "0"
// lambda_advanced.baml:161:5 (comment) len=54 "// body returns literal 0, U binds to 0 (literal type)"
// lambda_advanced.baml:162:5 (comment) len=59 "// result is 0[], function returns int[] — needs 0 <: int"
// lambda_advanced.baml:165:1 (comment) len=132 "// ── Compose: fully inferred lambdas ──────────────────────────────"
// lambda_advanced.baml:166:1 (keyword) len=8 "function"
// lambda_advanced.baml:166:10 (function) [declaration] len=21 "test_compose_inferred"
// lambda_advanced.baml:166:37 (type) [defaultLibrary] len=3 "int"
// lambda_advanced.baml:167:5 (keyword) len=3 "let"
// lambda_advanced.baml:167:9 (variable) [declaration] len=5 "items"
// lambda_advanced.baml:167:16 (type) [defaultLibrary] len=3 "int"
// lambda_advanced.baml:167:22 (operator) len=1 "="
// lambda_advanced.baml:167:25 (number) len=1 "1"
// lambda_advanced.baml:167:28 (number) len=1 "2"
// lambda_advanced.baml:167:31 (number) len=1 "3"
// lambda_advanced.baml:168:5 (keyword) len=3 "let"
// lambda_advanced.baml:168:9 (variable) [declaration] len=5 "step1"
// lambda_advanced.baml:168:16 (type) [defaultLibrary] len=3 "int"
// lambda_advanced.baml:168:22 (operator) len=1 "="
// lambda_advanced.baml:168:24 (variable) len=5 "items"
// lambda_advanced.baml:168:30 (method) len=3 "map"
// lambda_advanced.baml:168:35 (parameter) [declaration] len=1 "x"
// lambda_advanced.baml:168:43 (parameter) len=1 "x"
// lambda_advanced.baml:168:45 (operator) len=1 "*"
// lambda_advanced.baml:168:47 (number) len=1 "2"
// lambda_advanced.baml:169:5 (variable) len=5 "step1"
// lambda_advanced.baml:169:11 (method) len=3 "map"
// lambda_advanced.baml:169:16 (parameter) [declaration] len=1 "x"
// lambda_advanced.baml:169:24 (parameter) len=1 "x"
// lambda_advanced.baml:169:26 (operator) len=1 "+"
// lambda_advanced.baml:169:28 (number) len=1 "1"
// lambda_advanced.baml:170:5 (comment) len=58 "// Both lambdas: x inferred as int, return inferred as int"
// lambda_advanced.baml:173:1 (comment) len=102 "// ── Higher-order: lambda returning inferred lambda ───────────────"
// lambda_advanced.baml:174:1 (keyword) len=8 "function"
// lambda_advanced.baml:174:10 (function) [declaration] len=26 "test_higher_order_inferred"
// lambda_advanced.baml:174:42 (type) [defaultLibrary] len=3 "int"
// lambda_advanced.baml:175:5 (keyword) len=3 "let"
// lambda_advanced.baml:175:9 (variable) [declaration] len=10 "make_adder"
// lambda_advanced.baml:175:20 (operator) len=1 "="
// lambda_advanced.baml:175:23 (parameter) [declaration] len=1 "n"
// lambda_advanced.baml:175:26 (type) [defaultLibrary] len=3 "int"
// lambda_advanced.baml:176:9 (keyword) len=3 "let"
// lambda_advanced.baml:176:13 (variable) [declaration] len=5 "adder"
// lambda_advanced.baml:176:19 (operator) len=1 "="
// lambda_advanced.baml:176:22 (parameter) [declaration] len=1 "x"
// lambda_advanced.baml:176:25 (type) [defaultLibrary] len=3 "int"
// lambda_advanced.baml:176:35 (parameter) len=1 "x"
// lambda_advanced.baml:176:37 (operator) len=1 "+"
// lambda_advanced.baml:176:39 (parameter) len=1 "n"
// lambda_advanced.baml:177:9 (variable) len=5 "adder"
// lambda_advanced.baml:179:5 (comment) len=58 "// make_adder return type inferred from body: (int) -> int"
// lambda_advanced.baml:180:5 (keyword) len=3 "let"
// lambda_advanced.baml:180:9 (variable) [declaration] len=4 "add5"
// lambda_advanced.baml:180:14 (operator) len=1 "="
// lambda_advanced.baml:180:16 (variable) len=10 "make_adder"
// lambda_advanced.baml:180:27 (number) len=1 "5"
// lambda_advanced.baml:181:5 (variable) len=4 "add5"
// lambda_advanced.baml:181:10 (number) len=2 "37"
// lambda_advanced.baml:184:1 (comment) len=106 "// ── Lambda calling lambda: no return annotations ─────────────────"
// lambda_advanced.baml:185:1 (keyword) len=8 "function"
// lambda_advanced.baml:185:10 (function) [declaration] len=25 "test_apply_twice_inferred"
// lambda_advanced.baml:185:41 (type) [defaultLibrary] len=3 "int"
// lambda_advanced.baml:186:5 (keyword) len=3 "let"
// lambda_advanced.baml:186:9 (variable) [declaration] len=6 "double"
// lambda_advanced.baml:186:16 (operator) len=1 "="
// lambda_advanced.baml:186:19 (parameter) [declaration] len=1 "x"
// lambda_advanced.baml:186:22 (type) [defaultLibrary] len=3 "int"
// lambda_advanced.baml:186:32 (parameter) len=1 "x"
// lambda_advanced.baml:186:34 (operator) len=1 "*"
// lambda_advanced.baml:186:36 (number) len=1 "2"
// lambda_advanced.baml:187:5 (keyword) len=3 "let"
// lambda_advanced.baml:187:9 (variable) [declaration] len=11 "apply_twice"
// lambda_advanced.baml:187:21 (operator) len=1 "="
// lambda_advanced.baml:187:24 (parameter) [declaration] len=1 "f"
// lambda_advanced.baml:187:28 (type) [defaultLibrary] len=3 "int"
// lambda_advanced.baml:187:36 (type) [defaultLibrary] len=3 "int"
// lambda_advanced.baml:187:40 (keyword) len=6 "throws"
// lambda_advanced.baml:187:47 (type) [defaultLibrary] len=5 "never"
// lambda_advanced.baml:187:54 (parameter) [declaration] len=1 "x"
// lambda_advanced.baml:187:57 (type) [defaultLibrary] len=3 "int"
// lambda_advanced.baml:187:67 (parameter) len=1 "f"
// lambda_advanced.baml:187:69 (parameter) len=1 "f"
// lambda_advanced.baml:187:71 (parameter) len=1 "x"
// lambda_advanced.baml:188:5 (variable) len=11 "apply_twice"
// lambda_advanced.baml:188:17 (variable) len=6 "double"
// lambda_advanced.baml:188:25 (number) len=1 "3"
// lambda_advanced.baml:191:1 (comment) len=116 "// ── Deep capture with inferred return types ──────────────────────"
// lambda_advanced.baml:192:1 (keyword) len=8 "function"
// lambda_advanced.baml:192:10 (function) [declaration] len=26 "test_deep_capture_inferred"
// lambda_advanced.baml:192:42 (type) [defaultLibrary] len=3 "int"
// lambda_advanced.baml:193:5 (keyword) len=3 "let"
// lambda_advanced.baml:193:9 (variable) [declaration] len=1 "a"
// lambda_advanced.baml:193:11 (operator) len=1 "="
// lambda_advanced.baml:193:13 (number) len=1 "1"
// lambda_advanced.baml:194:5 (keyword) len=3 "let"
// lambda_advanced.baml:194:9 (variable) [declaration] len=1 "f"
// lambda_advanced.baml:194:11 (operator) len=1 "="
// lambda_advanced.baml:194:14 (parameter) [declaration] len=1 "b"
// lambda_advanced.baml:194:17 (type) [defaultLibrary] len=3 "int"
// lambda_advanced.baml:195:9 (keyword) len=3 "let"
// lambda_advanced.baml:195:13 (variable) [declaration] len=1 "g"
// lambda_advanced.baml:195:15 (operator) len=1 "="
// lambda_advanced.baml:195:18 (parameter) [declaration] len=1 "c"
// lambda_advanced.baml:195:21 (type) [defaultLibrary] len=3 "int"
// lambda_advanced.baml:195:31 (variable) len=1 "a"
// lambda_advanced.baml:195:33 (operator) len=1 "+"
// lambda_advanced.baml:195:35 (parameter) len=1 "b"
// lambda_advanced.baml:195:37 (operator) len=1 "+"
// lambda_advanced.baml:195:39 (parameter) len=1 "c"
// lambda_advanced.baml:196:9 (variable) len=1 "g"
// lambda_advanced.baml:196:11 (number) len=3 "100"
// lambda_advanced.baml:198:5 (variable) len=1 "f"
// lambda_advanced.baml:198:7 (number) len=2 "10"
// lambda_advanced.baml:201:1 (comment) len=126 "// ── Multiple lambda args: all inferred ───────────────────────────"
// lambda_advanced.baml:202:1 (keyword) len=8 "function"
// lambda_advanced.baml:202:10 (function) [declaration] len=28 "test_two_transforms_inferred"
// lambda_advanced.baml:202:44 (type) [defaultLibrary] len=3 "int"
// lambda_advanced.baml:203:5 (keyword) len=3 "let"
// lambda_advanced.baml:203:9 (variable) [declaration] len=10 "apply_both"
// lambda_advanced.baml:203:20 (operator) len=1 "="
// lambda_advanced.baml:203:23 (parameter) [declaration] len=1 "f"
// lambda_advanced.baml:203:27 (type) [defaultLibrary] len=3 "int"
// lambda_advanced.baml:203:35 (type) [defaultLibrary] len=3 "int"
// lambda_advanced.baml:203:39 (keyword) len=6 "throws"
// lambda_advanced.baml:203:46 (type) [defaultLibrary] len=5 "never"
// lambda_advanced.baml:203:53 (parameter) [declaration] len=1 "g"
// lambda_advanced.baml:203:57 (type) [defaultLibrary] len=3 "int"
// lambda_advanced.baml:203:65 (type) [defaultLibrary] len=3 "int"
// lambda_advanced.baml:203:69 (keyword) len=6 "throws"
// lambda_advanced.baml:203:76 (type) [defaultLibrary] len=5 "never"
// lambda_advanced.baml:203:83 (parameter) [declaration] len=1 "x"
// lambda_advanced.baml:203:86 (type) [defaultLibrary] len=3 "int"
// lambda_advanced.baml:203:96 (parameter) len=1 "g"
// lambda_advanced.baml:203:98 (parameter) len=1 "f"
// lambda_advanced.baml:203:100 (parameter) len=1 "x"
// lambda_advanced.baml:204:5 (variable) len=10 "apply_both"
// lambda_advanced.baml:205:10 (parameter) [declaration] len=1 "x"
// lambda_advanced.baml:205:13 (type) [defaultLibrary] len=3 "int"
// lambda_advanced.baml:205:23 (parameter) len=1 "x"
// lambda_advanced.baml:205:25 (operator) len=1 "*"
// lambda_advanced.baml:205:27 (number) len=1 "2"
// lambda_advanced.baml:206:10 (parameter) [declaration] len=1 "x"
// lambda_advanced.baml:206:13 (type) [defaultLibrary] len=3 "int"
// lambda_advanced.baml:206:23 (parameter) len=1 "x"
// lambda_advanced.baml:206:25 (operator) len=1 "+"
// lambda_advanced.baml:206:27 (number) len=2 "10"
// lambda_advanced.baml:207:9 (number) len=1 "5"
// lambda_advanced.baml:211:1 (comment) len=88 "// ── Lambda in match arms: return type unified across arms ────────"
// lambda_advanced.baml:212:1 (keyword) len=8 "function"
// lambda_advanced.baml:212:10 (function) [declaration] len=29 "test_lambda_in_match_inferred"
// lambda_advanced.baml:212:45 (type) [defaultLibrary] len=3 "int"
// lambda_advanced.baml:213:5 (keyword) len=3 "let"
// lambda_advanced.baml:213:9 (variable) [declaration] len=2 "op"
// lambda_advanced.baml:213:12 (operator) len=1 "="
// lambda_advanced.baml:213:14 (string) len=8 "\"double\""
// lambda_advanced.baml:214:5 (keyword) len=3 "let"
// lambda_advanced.baml:214:9 (variable) [declaration] len=1 "f"
// lambda_advanced.baml:214:11 (operator) len=1 "="
// lambda_advanced.baml:214:13 (keyword) len=5 "match"
// lambda_advanced.baml:214:20 (variable) len=2 "op"
// lambda_advanced.baml:215:9 (string) len=8 "\"double\""
// lambda_advanced.baml:215:22 (parameter) [declaration] len=1 "x"
// lambda_advanced.baml:215:25 (type) [defaultLibrary] len=3 "int"
// lambda_advanced.baml:215:35 (parameter) len=1 "x"
// lambda_advanced.baml:215:37 (operator) len=1 "*"
// lambda_advanced.baml:215:39 (number) len=1 "2"
// lambda_advanced.baml:216:9 (string) len=8 "\"negate\""
// lambda_advanced.baml:216:22 (parameter) [declaration] len=1 "x"
// lambda_advanced.baml:216:25 (type) [defaultLibrary] len=3 "int"
// lambda_advanced.baml:216:35 (number) len=1 "0"
// lambda_advanced.baml:216:37 (operator) len=1 "-"
// lambda_advanced.baml:216:39 (parameter) len=1 "x"
// lambda_advanced.baml:217:15 (parameter) [declaration] len=1 "x"
// lambda_advanced.baml:217:18 (type) [defaultLibrary] len=3 "int"
// lambda_advanced.baml:217:28 (parameter) len=1 "x"
// lambda_advanced.baml:219:5 (variable) len=1 "f"
// lambda_advanced.baml:219:7 (number) len=2 "21"
// lambda_advanced.baml:222:1 (comment) len=86 "// ── Capture + map: inferred capture type flows into lambda ───────"
// lambda_advanced.baml:223:1 (keyword) len=8 "function"
// lambda_advanced.baml:223:10 (function) [declaration] len=28 "test_capture_in_map_inferred"
// lambda_advanced.baml:223:44 (type) [defaultLibrary] len=3 "int"
// lambda_advanced.baml:224:5 (keyword) len=3 "let"
// lambda_advanced.baml:224:9 (variable) [declaration] len=5 "items"
// lambda_advanced.baml:224:16 (type) [defaultLibrary] len=3 "int"
// lambda_advanced.baml:224:22 (operator) len=1 "="
// lambda_advanced.baml:224:25 (number) len=1 "1"
// lambda_advanced.baml:224:28 (number) len=1 "2"
// lambda_advanced.baml:224:31 (number) len=1 "3"
// lambda_advanced.baml:225:5 (keyword) len=3 "let"
// lambda_advanced.baml:225:9 (variable) [declaration] len=6 "factor"
// lambda_advanced.baml:225:16 (operator) len=1 "="
// lambda_advanced.baml:225:18 (number) len=2 "10"
// lambda_advanced.baml:226:5 (variable) len=5 "items"
// lambda_advanced.baml:226:11 (method) len=3 "map"
// lambda_advanced.baml:226:16 (parameter) [declaration] len=1 "x"
// lambda_advanced.baml:226:24 (parameter) len=1 "x"
// lambda_advanced.baml:226:26 (operator) len=1 "*"
// lambda_advanced.baml:226:28 (variable) len=6 "factor"
// lambda_advanced.baml:227:5 (comment) len=56 "// x inferred as int, factor captured as int, return int"
// lambda_advanced.baml:230:1 (comment) len=138 "// ── Void lambda: no return value ─────────────────────────────────"
// lambda_advanced.baml:231:1 (keyword) len=8 "function"
// lambda_advanced.baml:231:10 (function) [declaration] len=25 "test_void_lambda_inferred"
// lambda_advanced.baml:231:41 (type) [defaultLibrary] len=3 "int"
// lambda_advanced.baml:232:5 (keyword) len=3 "let"
// lambda_advanced.baml:232:9 (variable) [declaration] len=5 "count"
// lambda_advanced.baml:232:15 (operator) len=1 "="
// lambda_advanced.baml:232:17 (number) len=1 "0"
// lambda_advanced.baml:233:5 (keyword) len=3 "let"
// lambda_advanced.baml:233:9 (variable) [declaration] len=3 "inc"
// lambda_advanced.baml:233:13 (operator) len=1 "="
// lambda_advanced.baml:233:23 (variable) len=5 "count"
// lambda_advanced.baml:233:29 (operator) len=2 "+="
// lambda_advanced.baml:233:32 (number) len=1 "1"
// lambda_advanced.baml:234:5 (variable) len=3 "inc"
// lambda_advanced.baml:235:5 (variable) len=3 "inc"
// lambda_advanced.baml:236:5 (variable) len=5 "count"
// lambda_advanced.baml:239:1 (keyword) len=8 "function"
// lambda_advanced.baml:239:10 (function) [declaration] len=34 "test_void_lambda_inferred_captures"
// lambda_advanced.baml:239:50 (type) [defaultLibrary] len=3 "int"
// lambda_advanced.baml:240:5 (keyword) len=3 "let"
// lambda_advanced.baml:240:9 (variable) [declaration] len=3 "inc"
// lambda_advanced.baml:240:13 (operator) len=1 "="
// lambda_advanced.baml:241:9 (keyword) len=3 "let"
// lambda_advanced.baml:241:13 (variable) [declaration] len=5 "count"
// lambda_advanced.baml:241:19 (operator) len=1 "="
// lambda_advanced.baml:241:21 (number) len=1 "0"
// lambda_advanced.baml:242:9 (keyword) len=3 "let"
// lambda_advanced.baml:242:13 (variable) [declaration] len=3 "inc"
// lambda_advanced.baml:242:17 (operator) len=1 "="
// lambda_advanced.baml:242:27 (variable) len=5 "count"
// lambda_advanced.baml:242:33 (operator) len=2 "+="
// lambda_advanced.baml:242:36 (number) len=1 "1"
// lambda_advanced.baml:242:39 (variable) len=5 "count"
// lambda_advanced.baml:243:9 (variable) len=3 "inc"
// lambda_advanced.baml:245:5 (variable) len=3 "inc"
// lambda_advanced.baml:246:5 (variable) len=3 "inc"
// lambda_advanced.baml:247:5 (variable) len=3 "inc"
// lambda_advanced.baml:251:1 (comment) len=132 "// ── Lambda capturing another lambda ──────────────────────────────"
// lambda_advanced.baml:252:1 (keyword) len=8 "function"
// lambda_advanced.baml:252:10 (function) [declaration] len=28 "test_capture_lambda_inferred"
// lambda_advanced.baml:252:44 (type) [defaultLibrary] len=3 "int"
// lambda_advanced.baml:253:5 (keyword) len=3 "let"
// lambda_advanced.baml:253:9 (variable) [declaration] len=8 "multiply"
// lambda_advanced.baml:253:18 (operator) len=1 "="
// lambda_advanced.baml:253:21 (parameter) [declaration] len=1 "a"
// lambda_advanced.baml:253:24 (type) [defaultLibrary] len=3 "int"
// lambda_advanced.baml:253:29 (parameter) [declaration] len=1 "b"
// lambda_advanced.baml:253:32 (type) [defaultLibrary] len=3 "int"
// lambda_advanced.baml:253:42 (parameter) len=1 "a"
// lambda_advanced.baml:253:44 (operator) len=1 "*"
// lambda_advanced.baml:253:46 (parameter) len=1 "b"
// lambda_advanced.baml:254:5 (keyword) len=3 "let"
// lambda_advanced.baml:254:9 (variable) [declaration] len=6 "double"
// lambda_advanced.baml:254:16 (operator) len=1 "="
// lambda_advanced.baml:254:19 (parameter) [declaration] len=1 "x"
// lambda_advanced.baml:254:22 (type) [defaultLibrary] len=3 "int"
// lambda_advanced.baml:254:32 (variable) len=8 "multiply"
// lambda_advanced.baml:254:41 (parameter) len=1 "x"
// lambda_advanced.baml:254:44 (number) len=1 "2"
// lambda_advanced.baml:255:5 (variable) len=6 "double"
// lambda_advanced.baml:255:12 (number) len=2 "21"
// lambda_advanced.baml:258:1 (comment) len=96 "// ── Map over optional array: param type int? inferred ────────────"
// lambda_advanced.baml:259:1 (keyword) len=8 "function"
// lambda_advanced.baml:259:10 (function) [declaration] len=26 "test_map_optional_inferred"
// lambda_advanced.baml:259:42 (type) [defaultLibrary] len=3 "int"
// lambda_advanced.baml:260:5 (keyword) len=3 "let"
// lambda_advanced.baml:260:9 (variable) [declaration] len=5 "items"
// lambda_advanced.baml:260:16 (type) [defaultLibrary] len=3 "int"
// lambda_advanced.baml:260:23 (operator) len=1 "="
// lambda_advanced.baml:260:26 (number) len=1 "1"
// lambda_advanced.baml:260:29 (type) [defaultLibrary] len=4 "null"
// lambda_advanced.baml:260:35 (number) len=1 "3"
// lambda_advanced.baml:261:5 (variable) len=5 "items"
// lambda_advanced.baml:261:11 (method) len=3 "map"
// lambda_advanced.baml:261:16 (parameter) [declaration] len=1 "x"
// lambda_advanced.baml:262:9 (keyword) len=5 "match"
// lambda_advanced.baml:262:16 (parameter) len=1 "x"
// lambda_advanced.baml:263:13 (type) [defaultLibrary] len=4 "null"
// lambda_advanced.baml:263:21 (number) len=1 "0"
// lambda_advanced.baml:264:18 (number) len=1 "1"
// lambda_advanced.baml:267:5 (comment) len=45 "// x: int? inferred from T in Array<int?>.map"
// lambda_advanced.baml:268:5 (comment) len=34 "// return: 0|1 inferred from match"
// lambda_advanced.baml:271:1 (comment) len=136 "// ── Shadowing with inferred types ────────────────────────────────"
// lambda_advanced.baml:272:1 (keyword) len=8 "function"
// lambda_advanced.baml:272:10 (function) [declaration] len=23 "test_shadowing_inferred"
// lambda_advanced.baml:272:39 (type) [defaultLibrary] len=3 "int"
// lambda_advanced.baml:273:5 (keyword) len=3 "let"
// lambda_advanced.baml:273:9 (variable) [declaration] len=1 "x"
// lambda_advanced.baml:273:11 (operator) len=1 "="
// lambda_advanced.baml:273:13 (string) len=14 "\"not a number\""
// lambda_advanced.baml:274:5 (keyword) len=3 "let"
// lambda_advanced.baml:274:9 (variable) [declaration] len=5 "items"
// lambda_advanced.baml:274:16 (type) [defaultLibrary] len=3 "int"
// lambda_advanced.baml:274:22 (operator) len=1 "="
// lambda_advanced.baml:274:25 (number) len=1 "1"
// lambda_advanced.baml:274:28 (number) len=1 "2"
// lambda_advanced.baml:274:31 (number) len=1 "3"
// lambda_advanced.baml:275:5 (variable) len=5 "items"
// lambda_advanced.baml:275:11 (method) len=3 "map"
// lambda_advanced.baml:275:16 (parameter) [declaration] len=1 "x"
// lambda_advanced.baml:275:24 (parameter) len=1 "x"
// lambda_advanced.baml:275:26 (operator) len=1 "+"
// lambda_advanced.baml:275:28 (number) len=1 "1"
// lambda_advanced.baml:276:5 (comment) len=60 "// lambda x: int (from map context), shadows outer x: string"
// lambda_advanced.baml:279:1 (comment) len=146 "// ── IIFE with inferred types ─────────────────────────────────────"
// lambda_advanced.baml:280:1 (keyword) len=8 "function"
// lambda_advanced.baml:280:10 (function) [declaration] len=18 "test_iife_inferred"
// lambda_advanced.baml:280:34 (type) [defaultLibrary] len=3 "int"
// lambda_advanced.baml:281:7 (parameter) [declaration] len=1 "x"
// lambda_advanced.baml:281:10 (type) [defaultLibrary] len=3 "int"
// lambda_advanced.baml:281:20 (parameter) len=1 "x"
// lambda_advanced.baml:281:22 (operator) len=1 "*"
// lambda_advanced.baml:281:24 (parameter) len=1 "x"
// lambda_advanced.baml:281:29 (number) len=1 "7"
