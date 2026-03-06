// ── Array Literals ──────────────────────────────────────────────

function MakeArray() -> int[] {
    return [1, 2, 3];
}

function EmptyArray() -> int[] {
    return [];
}

// mixed array → union
function MixedArray() -> string {
    let arr = [1, "two", 3];
    return "done";
}

// ── Map Literals ────────────────────────────────────────────────

function MakeMap() -> map<string, int> {
    return {"a": 1, "b": 2};
}

// ── Index Access ────────────────────────────────────────────────

function IndexArray(arr: int[]) -> int {
    return arr[0];
}

function IndexMap(m: map<string, int>) -> int {
    return m["key"];
}
