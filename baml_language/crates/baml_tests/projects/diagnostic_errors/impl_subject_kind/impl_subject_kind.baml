// Only a concrete type can implement an interface (E0138). The judgment is
// made once, on the LOWERED for-target with alias heads expanded — and
// deliberately NOT on the normalized one.
//
// That gap is the whole point. Normalizing would collapse `true | false` to
// `bool`, making it a valid subject to this gate while coherence still saw a
// raw union — the two would disagree about the same block, which is exactly
// how a local `implement I for true | false` and a dependency's
// `implement I for bool` once escaped coherence with no diagnostic from
// either. Expanding aliases is not the same thing: an alias is a spelling
// device for the type it names, so it denotes the very same implementor.

interface Flagged {
    function flag(self) -> string throws never
}

class Ok {
}

class Aliased {
}

type AliasedName = Aliased

/// E0138: a union spelling, even one that would denote `bool`.
implement Flagged for true | false {
    function flag(self) -> string throws never {
        "union"
    }
}

/// E0138: `T?` is `T | null`.
implement Flagged for int? {
    function flag(self) -> string throws never {
        "optional"
    }
}

/// Accepted: an alias names the same implementor its definition does.
implement Flagged for AliasedName {
    function flag(self) -> string throws never {
        "alias"
    }
}

/// Accepted: the subject written as the concrete class itself.
implement Flagged for Ok {
    function flag(self) -> string throws never {
        "ok"
    }
}
