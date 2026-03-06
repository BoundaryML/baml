// ── Complex Type Flows ──────────────────────────────────────────

// result of one call determines the argument type of the next
function Pipeline(x: int) -> string {
    let a = Add4(x);
    let b = MakePoint(a, a);
    let c = b.x + b.y;
    let d = IntToString(c);
    return d;
}

// build an array from function call results
function CollectResults() -> int[] {
    let a = Add1(1);
    let b = Add2(2);
    let c = Add4(4);
    return [a, b, c];
}

// index into an array built from function results, pass to another function
function CollectAndUse() -> int {
    let results = CollectResults();
    return Add1(results[0]) + Add1(results[2]);
}

// ── Errors in Deep Chains ───────────────────────────────────────

// passing wrong arg type deep in chain
function BadFactorialCall() -> int {
    return Factorial("five");
}

// wrong return type propagated through chain
function BadPipelineReturn() -> bool {
    return Pipeline(1);
}

// function returns int, used where string[] expected
function BadCollect() -> string[] {
    return CollectResults();
}

// ── Evolving Containers + Function Calls ────────────────────────

// push results from function calls into evolving array
function EvolveFromCalls() -> int {
    let arr = [];
    arr.push(Add1(0));
    arr.push(Add2(0));
    arr.push(Add4(0));
    return arr[0];
}

// push wrong type from a function call → error
function EvolveMismatchFromCall() -> int {
    let arr = [];
    arr.push(Add1(0));
    arr.push(IntToString(0));
    return arr[0];
}
