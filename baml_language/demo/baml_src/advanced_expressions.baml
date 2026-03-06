// ── If/Else with Function Calls ─────────────────────────────────

function BranchCalls(useAdd1: bool, x: int) -> int {
    if (useAdd1) {
        Add1(x)
    } else {
        Add4(x)
    }
}

// branches return different types → widened to union
function BranchDifferentTypes(asInt: bool, x: int) -> int | string {
    if (asInt) {
        Add1(x)
    } else {
        IntToString(x)
    }
}

// ── Match with Function Calls ───────────────────────────────────

function MatchAndCall(c: Color) -> int {
    let base = match (c) {
        Color.Red => 10,
        Color.Green => 20,
        Color.Blue => 30,
    };
    return Add4(base);
}

function MatchAndCall(c: Color) -> int {
    let base = match (c) {
        Color.Red => 10,
        Color.Green => 20,
        Color.Blue => 30,
    };
    return Add4(base);
}

function MatchCallChain(c: Color) -> int {
    match (c) {
        Color.Red => Add1(0),
        Color.Green => Add2(0),
        Color.Blue => Add4(0),
    }
}

// ── Loop with Function Calls ────────────────────────────────────

function LoopWithCalls() -> int {
    let x = 0;
    let i = 0;
    while (i < 5) {
        x = Add2(x);
        i += 1;
    }
    return x;
}

// accumulate function results in evolving array
function LoopCollect() -> int {
    let results = [];
    let i = 0;
    while (i < 3) {
        results.push(Add1(i));
        i += 1;
    }
    return results[0];
}

// ── Deeply Nested Call Expressions ──────────────────────────────

// f(g(h(x))) + f(g(h(y)))
function DeepNest(x: int, y: int) -> int {
    return Add1(Add2(Add4(x))) + Add1(Add2(Add4(y)));
}

// function call in array literal in function call argument
function CallInArrayInCall() -> int {
    let arr = [Add1(1), Add2(2), Add4(4)];
    return arr[0] + arr[1] + arr[2];
}

// function call as map value
function CallInMap() -> map<string, int> {
    return {"a": Add1(0), "b": Add2(0), "c": Add4(0)};
}

// function result used as condition
function CallAsCondition(x: int) -> string {
    let x = if (Comparison(x, 10)) {
        return "less";
    } + 10;
    return "not less";
}

// function result used in match scrutinee
function CallInMatchScrutinee(x: int) -> string {
    match (PickColor(x)) {
        Color.Red => "warm",
        Color.Green => "cool",
        Color.Blue => "cool",
    }
}
