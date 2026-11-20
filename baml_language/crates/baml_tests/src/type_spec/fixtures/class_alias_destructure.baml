// A destructure through an alias of a class destructures that class: its
// fields type as the class's, at the arguments the alias pins, and the
// arm covers the class.
class CadFoo {
    v int
}

class CadBox<T> {
    value T
}

type CadFooAlias = CadFoo
type CadFooChain = CadFooAlias
type CadIntBox = CadBox<int>

function cad_alias(x: CadFoo) -> int {
    let r = match (x) {
     // ^ int
        CadFooAlias { v: let n } => n
     //                      ^ int
    }
    r
}

function cad_chain(x: CadFoo) -> int {
    let r = match (x) {
     // ^ int
        CadFooChain { v: let n } => n
     //                      ^ int
    }
    r
}

function cad_generic_target(b: CadBox<int>) -> int {
    let r = match (b) {
     // ^ int
        CadIntBox { value: let n } => n
     //                        ^ int
    }
    r
}

// The alias arm takes the class member of a union; the rest is covered by
// its own arm.
function cad_alias_in_union(x: CadFoo | null) -> int {
    let r = match (x) {
     // ^ int
        CadFooAlias { v: _ } => 1,
        null => 2
    }
    r
}

// A refutable field leaves the class uncovered.
function cad_alias_refutable(x: CadFoo) -> int {
    let r = match (x) { CadFooAlias { v: 1 } => 1 };
     // ^ !error
//          ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ non-exhaustive
    r
}

// A `let` destructure informs its initializer: through an alias, the
// demanded class is at the alias's arguments, which solve a generic call
// named only by its result.
class CadMarker<T> {
}

type CadIntMarker = CadMarker<int>

function cad_make<T>() -> CadMarker<T> {
    CadMarker<T> {}
}

function cad_let_alias_solves_the_call() -> int {
    let CadIntMarker {} = cad_make();
    //                    ^^^^^^^^^^ user.CadMarker<int>
    1
}

function cad_let_class_solves_the_call() -> int {
    let CadMarker<int> {} = cad_make();
    //                      ^^^^^^^^^^ user.CadMarker<int>
    1
}
