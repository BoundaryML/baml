// Interface-qualified method refs + interface destructures
//
// `IqDescribable.describe` is Rust's `Trait::method` (r-a's
// value-namespace trait path): interface methods are ordinary items,
// so the same signature road serves - `Self` instantiates fresh, its
// implements-bound rides along, and the call's first argument solves
// it. And `IqAnimal { name }` destructures an EXISTENTIAL by its
// declared fields, each typed through the same member instantiation
// field access uses.
interface IqDescribable {
    function describe(self) -> string throws never {
        "thing"
    }
}

class IqThing {
    implements IqDescribable {}
}

function iq_ref(t: IqThing) -> string throws never {
    let describe = IqDescribable.describe;
    let s = describe(t);
     // ^ string
    s
}

function iq_direct(t: IqThing) -> string throws never {
    IqDescribable.describe(t)
}

interface IqAnimal {
    name: string
}

class IqDog {
    name string

    implements IqAnimal {}
}

function iq_destructure(a: IqAnimal) -> string throws never {
    match (a) {
        IqAnimal { name } => name
    }
}

function iq_use(d: IqDog) -> string throws never {
    iq_destructure(d) + iq_ref(IqThing {})
}
