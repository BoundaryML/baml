class C0 {}
class C1 {}

function originalGrouped(h: int | C1) -> string {
    let g: (int | string) | C0 = h;
    if (!(g is int) && !(g is string)) {
        let c: C0 = g;
        return reflect.Type.of_value(c).to_string();
    }
    "other"
}

function originalFlat(h: int | C1) -> string {
    let g: int | string | C0 = h;
    if (!(g is int) && !(g is string)) {
        let c: C0 = g;
        return reflect.Type.of_value(c).to_string();
    }
    "other"
}

function groupedClass(h: int | C1) -> int {
    let g: (int | string) | C0 {} = h;
    0
}

function flatClass(h: int | C1) -> int {
    let g: int | string | C0 {} = h;
    0
}

function groupedNested(h: int | C1) -> int {
    let g: let k: (int | string) | C0 = h;
    0
}

function flatNested(h: int | C1) -> int {
    let g: let k: int | string | C0 = h;
    0
}

function disjointPlain(h: C1) -> int {
    match (h) {
        let g: int => 1,
        _ => 0,
    }
}

function disjointFlat(h: C1) -> int {
    match (h) {
        let g: int | string | C0 => 1,
        _ => 0,
    }
}

function disjointGrouped(h: C1) -> int {
    match (h) {
        let g: (int | string) | C0 => 1,
        _ => 0,
    }
}

function primaryError() -> int {
    let g: let k: int = missingValue;
    0
}

class Tagged { tag: int }

function constrainedAlternative(h: int | Tagged) -> int {
    let g: int | Tagged { tag: 1 } = h;
    0
}

function narrowedList(h: int[] | string) -> int {
    let xs: [..]: int[] = h;
    0
}

class Flag { tag: bool }

function missingBooleanAlternative(h: int | Flag) -> int {
    let g: int | Flag { tag: true } = h;
    0
}

function narrowedElements(h: (int | string)[]) -> int {
    let xs: [..]: int[] = h;
    0
}

type Either = C0 | C1;

function partialAlias(h: Either) -> int {
    let g: C0 {} = h;
    0
}

function partialDirect(h: C0 | C1) -> int {
    let g: C0 {} = h;
    0
}

function unresolvedField(h: Tagged) -> int {
    let g: Tagged { tag: let n: Missing } = h;
    0
}

function disjointBinding(h: C1) -> int {
    let g: let k: C0 = h;
    0
}

function refutableLength(h: int[]) -> int {
    let xs: [let first, let second] = h;
    0
}

function tupleAlternativeNeverCoversClass(h: int | C0) -> int {
    let g: int | (C0 {}, C0 {}) = h;
    0
}

function impossibleListField(h: Tagged) -> int {
    let g: Tagged { tag: [..] } = h;
    0
}

class C2 {}
type Three = C0 | C1 | C2

function unnamedDirect(h: C0 | C1 | C2) -> int {
    let C0 {} | C1 {} = h;
    0
}
function unnamedAlias(h: Three) -> int {
    let C0 {} | C1 {} = h;
    0
}
function forDirect(h: C0 | C1 | C2) -> int {
    for (let g: C0 {} | C1 {} in [h]) { return 1; }
    0
}
function forAlias(h: Three) -> int {
    for (let g: C0 {} | C1 {} in [h]) { return 1; }
    0
}

function disjointScalar(h: string) -> int {
    match (h) {
        let g: int => 1,
        _ => 0,
    }
}

function disjointTuple(h: string) -> int {
    match (h) {
        (let a: int, let b: int) => 1,
        _ => 0,
    }
}

function disjointList(h: string[]) -> int {
    match (h) {
        [_, ..]: int[] => 1,
        _ => 0,
    }
}

function uncoveredTupleAlternative(h: int | C0) -> int {
    match (h) {
        let g: int | (C0 {}, C0 {}) => 1,
    }
}
