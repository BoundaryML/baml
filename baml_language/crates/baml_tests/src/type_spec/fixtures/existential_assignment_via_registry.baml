//
// (The join lines: TIR joins are dedup-only and keep the member
// alongside its interface; the canonical algebra absorbs.)
//
// I1: the impl registry answers `C <: I` through the shared algebra -
// a class with an in-body `implements` block (normalized to the free
// shape) assigns to the interface existential, and the union of a
// member with its interface absorbs to the interface.
interface Greet {
    function hello(self) -> string throws never
}

class Guy {
    name string
    implements Greet {
        function hello(self) -> string throws never {
            "hi"
        }
    }
}

function f(g: Guy) -> Greet {
    let e: Greet = g;
     // ^ user.Greet
    e
}

// The registry-observable half: the JOIN of a member with its
// interface absorbs to the interface only if `Guy <: Greet` actually
// proves through the impl registry.
function join_absorbs(g: Guy, e: Greet, c: bool) -> Greet {
    let u = if (c) { g } else { e };
     // ^ user.Greet
    u
}
