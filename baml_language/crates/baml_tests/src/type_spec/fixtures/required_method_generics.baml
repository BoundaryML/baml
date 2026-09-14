// Required interface methods are ordinary functions
//
// A REQUIRED (bodyless) method is the same item kind as a default
// method - r-a's shape, where the has-body distinction matters only to
// body lowering. `seen<U>` on an existential receiver instantiates its
// own generic from the call site like any other method.
interface RmProbe {
    function seen<U>(self, value: U) -> string throws never
}

class RmA {
    implements RmProbe {
        function seen<U>(self, value: U) -> string throws never {
            "a"
        }
    }
}

function rm_dispatch(p: RmProbe) -> string throws never {
    let s = p.seen(1);
     // ^ string
    s
}

function rm_use() -> string throws never {
    rm_dispatch(RmA {})
}
