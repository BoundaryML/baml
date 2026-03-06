// ── Evolving Containers ────────────────────────────────────────

// empty array → first push establishes type
function EvolvingArray() -> int {
    let x = [];
    x.push(5);
    x.push(10);
    return x[0];
}

// push wrong type after establishing → error
function EvolvingArrayMismatch() -> int {
    let x = [];
    x.push(5);
    x.push("oops");
    return x[0];
}

// index assignment establishes type
function ArrayIndexAssign() -> int {
    let x = [];
    x[0] = 42;
    x[1] = 100;
    return x[0];
}

// empty map → first bracket assign establishes key+value type
function EvolvingMap() -> int {
    let m = {};
    m["a"] = 1;
    m["b"] = 2;
    return m["a"];
}

// map wrong value type after establishing → error
function EvolvingMapMismatch() -> int {
    let m = {};
    m["a"] = 1;
    m["b"] = "oops";
    return m["a"];
}

// annotated array — push matching type ok
function AnnotatedArrayPush() -> int {
    let x: int[] = [];
    x.push(5);
    return x[0];
}

// annotated array — push wrong type → error
function AnnotatedArrayPushWrong() -> int {
    let x: int[] = [];
    x.push("oops");
    return x[0];
}

// unannotated array — push wrong type → error
function UnannotatedArrayPushWrong() -> int {
    let x = [];
    if (true) {
      x.push("oops");
    }
    x.push(1);
    return x[0];
}
