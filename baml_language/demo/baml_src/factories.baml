// ── Functions Returning Classes ─────────────────────────────────

function MakePoint(x: int, y: int) -> Point {
    return Point { x: x, y: y };
}

// call function that returns class, then access field
function PointX(a: int, b: int) -> int {
    return MakePoint(a, b).x;
}

// nested: function returns class, field used in another call
function PointSum(a: int, b: int) -> int {
    let p = MakePoint(a, b);
    return Add1(p.x) + Add1(p.y);
}

// chained field access through multiple function calls
function MakePerson(n: string, a: int) -> Person {
    return Person { name: n, age: a, active: true };
}

function GetAge(n: string, a: int) -> int {
    return MakePerson(n, a).age;
}

// ── Functions Returning Arrays/Maps + Indexing ──────────────────

function MakeIntArray(a: int, b: int, c: int) -> int[] {
    return [a, b, c];
}

function FirstElement(a: int, b: int, c: int) -> int {
    return MakeIntArray(a, b, c)[0];
}

// function returns map, caller indexes into it
function MakeConfig() -> map<string, int> {
    return {"a": 1, "b": 2};
}

function GetTimeout() -> int {
    return MakeConfig()["timeout"];
}

// ── Functions Returning Enums ───────────────────────────────────

function PickColor(x: int) -> Color {
    if (x > 0) { Color.Red } else { Color.Blue }
}

function ColorName(x: int) -> string {
    match (PickColor(x)) {
        Color.Red => "red",
        Color.Green => "green",
        Color.Blue => "blue",
    }
}
