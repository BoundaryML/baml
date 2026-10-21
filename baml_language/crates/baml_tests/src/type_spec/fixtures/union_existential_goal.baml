// Union values against a VAR-carrying interface expectation
//
// (TIR types the call `unknown` - the spec is ahead here.)
//
// `int | string <: UgBox<?T>` decomposes into per-member Implements
// goals (spec Variance 2.1: a union is a subtype of an existential iff
// ALL members are; only concrete types implement, so a whole-union
// goal would fail at the registry even when every member holds). Each
// member confirms through its own impl and the confirmations agree on
// `?T := int`. Generic BOUNDS (`T extends I`) still reject unions -
// the concreteness rule - because bound goals register undecomposed.
interface UgBox<T> {
    function get(self) -> T throws never
}

implements UgBox<int> for int {
    function get(self) -> int throws never {
        self
    }
}

implements UgBox<int> for string {
    function get(self) -> int throws never {
        0
    }
}

function ug_get<T>(b: UgBox<T>) -> T throws never {
    b.get()
}

function ug_use(v: int | string) -> int throws never {
    let n = ug_get(v);
     // ^ int
    n
}
