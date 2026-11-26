// A spread in a map literal (`{ k: v, ...m }`) contributes its operand's KEY
// and VALUE types exactly as an entry of those types would: joined with the
// other members when the literal synthesizes, checked against the expected
// key and value when the literal has an expectation.
//
// The literal is a fresh map that the operand's entries are copied into, so
// it never aliases the operand, and `{ ...dogs }` may be a
// `map<string, MsAnimal>` although `map<string, MsDog>` is not one (spec:
// Variance - generics on maps are invariant).
interface MsAnimal {
    function name(self) -> string throws never
}

class MsDog {
    tag: string

    implements MsAnimal {
        function name(self) -> string throws never {
            self.tag
        }
    }
}

enum MsColor {
    Red
    Green
}

type MsDict = map<string, int>

// No expectation: key and value types are the joins of what every member
// contributes.
function synthesized(
    m: map<string, int>,
    names: map<string, string>,
    d: MsDict,
    by_int: map<int, bool>,
) -> int throws never {
    let a = { ...m };
     // ^ map<string, int>
    let b = { ...m, "k": 1 };
     // ^ map<string, int>
    let c = { ...m, ...names };
     // ^ map<string, int | string>
    let e = { ...d, "k": "text" };
     // ^ map<string, int | string>
    let f = { ...by_int, 3: true };
     // ^ map<int, bool>
    let g = { ...by_int, ...m };
     // ^ map<int | string, int | bool>
    let h = { ...{ ...m }, ...{ "k": 1 } };
     // ^ map<string, int>
    0
}

// A literal key type is taken as declared and joins as any key of that type.
function literal_keys(tags: map<"a" | "b", int>, colors: map<MsColor, int>) -> int throws never {
    let same = { ...tags };
     // ^^^^ map<"a" | "b", int>
    let joined = { ...tags, "c": 3 };
     // ^^^^^^ map<string, int>
    let pinned: map<"a" | "b" | "c", int> = { ...tags, "c": 3 };
     // ^^^^^^ map<"a" | "b" | "c", int>
    let by_color = { ...colors, MsColor.Green: 2 };
     // ^^^^^^^^ map<user.MsColor, int>
    0
}

// With an expectation, the literal IS the expected map type and each
// operand's key and value types are checked against the expected ones.
function expected(dogs: map<string, MsDog>, m: map<string, int>) -> int throws never {
    let animals: map<string, MsAnimal> = { ...dogs };
     // ^^^^^^^ map<string, user.MsAnimal>
    let wide: map<string, int | string> = { ...m, "s": "text" };
     // ^^^^ map<string, int | string>
    let j: baml.json.json = { ...m, "nothing": null };
     // ^ baml.json.json
    let unknowns: map<string, unknown> = { ...m, ...dogs };
     // ^^^^^^^^ map<string, unknown>
    0
}

// The operand itself is still an invariant map: only a spread copies.
function operand_stays_invariant(dogs: map<string, MsDog>) -> map<string, MsAnimal> throws never {
    let aliased: map<string, MsAnimal> = dogs;
    //                                   ^^^^ mismatch: expected map<string, user.MsAnimal>, got map<string, user.MsDog>
    { ...dogs }
}

// A key or value type that does not fit the expected one is a mismatch at
// the operand. A wide key type cannot be spread into a literal-keyed map:
// the entries it holds are not known to have one of those keys.
function entry_mismatch(
    names: map<string, string>,
    by_int: map<int, int>,
    m: map<string, int>,
) -> map<string, int> throws never {
    let keys: map<string, int> = { ...by_int };
    //                                ^^^^^^ mismatch: expected string, got int
    let tags: map<"a" | "b", int> = { "a": 1, ...m };
    //                                           ^ mismatch: expected "a" | "b", got string
    { "k": 1, ...names }
    //           ^^^^^ mismatch: expected int, got string
}

// An alias of a map type is a map type.
function aliases(d: MsDict) -> MsDict throws never {
    let a = { ...d };
     // ^ map<string, int>
    let b: MsDict = { ...d, "k": 1 };
     // ^ user.MsDict
    b
}

// Type parameters are ordinary key and value types.
function generic<K extends baml.Hash & baml.ops.Equals, V>(
    a: map<K, V>,
    b: map<K, V>,
) -> map<K, V> throws never {
    let out = { ...a, ...b };
     // ^^^ map<K, V>
    out
}

// Flow narrowing gives the operand its map type.
function narrowed(m: map<string, int>?) -> map<string, int> throws never {
    if (m != null) {
        let a = { ...m };
         // ^ map<string, int>
        return a;
    }
    let b = { ...(m ?? {}) };
     // ^ map<string, int>
    b
}

// The operand's throws propagate as any subexpression's do.
function fails() -> map<string, int> throws string {
    throw "no"
}

function propagates() -> map<string, int> {
    let out = { "k": 0, ...fails() };
     // ^^^ map<string, int>
    out
}
