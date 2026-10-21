// Structural size of types, for the sampler's size penalties. Measured on the
// model rather than on rendered text so names cost nothing, and available
// before anything is rendered or compiled. Every measure is a fold over
// `children`, so adding a `Ty` variant is decided once, there.

/// Nodes in the type tree. An alias reference counts as one node; its
/// declaration is measured separately if at all.
function size(ty: Ty) -> int throws never {
    1
        + children(ty).reduce(
            (acc, c) -> {
                acc + size(c)
            },
            0,
        )
}

/// Nesting depth: how many constructors sit above the deepest leaf.
function depth(ty: Ty) -> int throws never {
    1
        + children(ty).reduce(
            (acc, c) -> {
                acc.max(depth(c))
            },
            0,
        )
}

/// The widest union anywhere in the type, or zero without one.
function union_width(ty: Ty) -> int throws never {
    let own = match (ty) {
        let u: Union => u.members().length(),
        Prim | Never | Top | Lit | EnumLit | Array | Map | Fn | Class | Enum | Iface | Alias | Var => {
            0
        },
    };
    children(ty).reduce(
        (acc, c) -> {
            acc.max(union_width(c))
        },
        own,
    )
}
