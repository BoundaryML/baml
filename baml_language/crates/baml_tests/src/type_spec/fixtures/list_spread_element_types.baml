// A spread in a list literal (`[a, ...xs]`) contributes its operand's
// ELEMENT type exactly as a plain element of that type would: joined with
// the other elements when the literal synthesizes, checked against the
// expected element when the literal has an expectation.
//
// The literal is a fresh list that the operand's elements are copied into,
// so it never aliases the operand. That is why `[...dogs]` may be an
// `LsAnimal[]` although `LsDog[]` is not one (spec: Variance - generics on
// lists are invariant): the check is `LsDog <: LsAnimal`, element by
// element, never `LsDog[] <: LsAnimal[]`.
interface LsAnimal {
    function name(self) -> string throws never
}

class LsDog {
    tag: string

    implements LsAnimal {
        function name(self) -> string throws never {
            self.tag
        }
    }
}

class LsCat {
    tag: string

    implements LsAnimal {
        function name(self) -> string throws never {
            self.tag
        }
    }
}

type LsInts = int[]
type LsNest = LsNest[]

// No expectation: the literal's element type is the join of what every
// element and every spread operand contributes.
function synthesized(xs: int[], names: string[], ys: LsInts, dogs: LsDog[]) -> int throws never {
    let a = [...xs];
     // ^ int[]
    let b = [0, ...xs, 3];
     // ^ int[]
    let c = [...xs, ...names];
     // ^ (int | string)[]
    let d = [...ys, 1];
     // ^ int[]
    let e = [...dogs];
     // ^ user.LsDog[]
    let f = [...dogs, LsCat { tag: "c" }];
     // ^ (user.LsCat | user.LsDog)[]
    let g = [...[1, 2], ...[...xs]];
     // ^ int[]
    let h = [[...xs], ...[names]];
     // ^ (int[] | string[])[]
    0
}

// An operand's element type is taken as declared: a literal element type is
// not widened by the spread itself, and joins as any element of that type.
function literal_elements(tags: ("a" | "b")[]) -> int throws never {
    let same = [...tags];
     // ^^^^ ("a" | "b")[]
    let joined = [...tags, "c"];
     // ^^^^^^ string[]
    let pinned: ("a" | "b" | "c")[] = [...tags, "c"];
     // ^^^^^^ ("a" | "b" | "c")[]
    0
}

// With an expectation, the literal IS the expected list type and each
// operand's element type is checked against the expected element.
function expected(dogs: LsDog[], xs: int[]) -> int throws never {
    let animals: LsAnimal[] = [...dogs];
     // ^^^^^^^ user.LsAnimal[]
    let fresh = [...dogs];
    let animals2: LsAnimal[] = [LsCat { tag: "c" }, ...fresh];
     // ^^^^^^^^ user.LsAnimal[]
    let wide: (int | string)[] = [...xs, "s"];
     // ^^^^ (int | string)[]
    let optional: int?[] = [...xs, null];
     // ^^^^^^^^ (int | null)[]
    let j: baml.json.json = [...xs, null];
     // ^ baml.json.json
    let unknowns: unknown[] = [...xs, ...dogs];
     // ^^^^^^^^ unknown[]
    0
}

// The operand itself is still an invariant list: only a spread copies.
function operand_stays_invariant(dogs: LsDog[]) -> LsAnimal[] throws never {
    let aliased: LsAnimal[] = dogs;
    //                        ^^^^ mismatch: expected user.LsAnimal[], got user.LsDog[]
    [...dogs]
}

// An operand whose elements do not fit the expected element is a mismatch
// at the operand, between the element types.
function element_mismatch(names: string[], dogs: LsDog[]) -> int[] throws never {
    let cats: LsCat[] = [...dogs];
    //                      ^^^^ mismatch: expected user.LsCat, got user.LsDog
    let tags: ("a" | "b")[] = [...names];
    //                            ^^^^^ mismatch: expected "a" | "b", got string
    [1, ...names]
    //     ^^^^^ mismatch: expected int, got string
}

// An alias of a list type is a list type; a recursive alias exposes its
// root (`LsNest` is `LsNest[]`).
function aliases(ys: LsInts, n: LsNest) -> int throws never {
    let a = [...ys];
     // ^ int[]
    let b = [...n];
     // ^ user.LsNest[]
    let c: LsNest = [...n, []];
     // ^ user.LsNest
    0
}

// A type parameter is an ordinary element type.
function generic<T>(xs: T[], x: T) -> T[] throws never {
    let out = [x, ...xs];
     // ^^^ T[]
    out
}

// An empty literal operand has a fresh element variable, solved from its
// siblings or from the expectation like any other open element.
function open_operands() -> int throws never {
    let a = [...[], 1];
     // ^ int[]
    let b: string[] = [...[]];
     // ^ string[]
    0
}

// Flow narrowing gives the operand its list type.
function narrowed(xs: int[]?) -> int[] throws never {
    if (xs != null) {
        let a = [...xs];
         // ^ int[]
        return a;
    }
    let b = [...(xs ?? [])];
     // ^ int[]
    b
}

// The operand's throws propagate as any subexpression's do.
function fails() -> int[] throws string {
    throw "no"
}

function propagates() -> int[] {
    let out = [0, ...fails()];
     // ^^^ int[]
    out
}

function diverges() -> never throws string {
    throw "never"
}

// `never` has no values: a diverging operand contributes nothing.
function never_operand() -> int[] throws string {
    let out = [1, ...diverges()];
     // ^^^ int[]
    out
}
