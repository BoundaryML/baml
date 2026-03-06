// ── If/Else ─────────────────────────────────────────────────────

// if/else join: both branches same type
function IfElseSame(flag: bool) -> int {
    if (flag) { 1 } else { 2 }
}

// if/else join: branches produce different literal values → widened to int
function IfElseWidened(flag: bool) -> int {
    let x = if (flag) { 1 } else { 2 };
    return x;
}

// if without else → optional
function IfNoElse(flag: bool) -> int? {
    if (flag) { 42 }
}

// ── Blocks & Tail Expressions ───────────────────────────────────

function BlockTailExpr() -> int {
    let x = {
        let a = 1;
        let b = 2;
        a + b
    };
    return x;
}

// nested blocks
function NestedBlocks() -> string {
    let result = {
        let inner = {
            "hello"
        };
        inner
    };
    return result;
}

// ── While / For / Break / Continue ──────────────────────────────

function WhileLoop() -> int {
    let x = 0;
    while (x < 10) {
        x += 1;
    }
    return x;
}

function ForLoop() -> int {
    let sum = 0;
    for (i in [1, 2, 3, 4, 5]) {
        sum += i;
    }
    return sum;
}

function EarlyBreak() -> int {
    let x = 0;
    while (true) {
        x += 1;
        if (x > 5) {
            break;
        }
    }
    return x;
}

// ── Dead Code Detection ─────────────────────────────────────────

function DeadCode() -> int {
    return 1;
    let x = 2;
}
