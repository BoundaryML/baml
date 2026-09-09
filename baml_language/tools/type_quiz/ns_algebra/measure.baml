// Structural size of types, for the sampler's size penalties. Measured on the
// model rather than on rendered text so names cost nothing, and available
// before anything is rendered or compiled.

/// Nodes in the type tree. An alias reference counts as one node; its
/// declaration is measured separately if at all.
function size(ty: Ty) -> int throws never {
    match (ty) {
        Prim => 1,
        Never => 1,
        Top => 1,
        Lit => 1,
        EnumLit => 1,
        let u: Union => {
            1
                + u.members.reduce(
                    (acc, m) -> {
                        acc + size(m)
                    },
                    0,
                )
        },
        let a: Array => 1 + size(a.element),
        let m: Map => 1 + size(m.key) + size(m.value),
        let f: Fn => {
            1
                + f.params.reduce(
                    (acc, p) -> {
                        acc + size(param_ty(p))
                    },
                    0,
                )
                + size(f.ret)
                + size(f.error)
        },
        let c: Class => {
            1
                + c.args.reduce(
                    (acc, a) -> {
                        acc + size(a)
                    },
                    0,
                )
        },
        Enum => 1,
        let i: Iface => {
            1
                + i.args.reduce(
                    (acc, a) -> {
                        acc + size(a)
                    },
                    0,
                )
                + i.pins.reduce(
                    (acc, p) -> {
                        acc + size(p.ty)
                    },
                    0,
                )
        },
        Alias => 1,
        Var => 1,
    }
}

/// Nesting depth: how many constructors sit above the deepest leaf.
function depth(ty: Ty) -> int throws never {
    match (ty) {
        Prim => 1,
        Never => 1,
        Top => 1,
        Lit => 1,
        EnumLit => 1,
        let u: Union => {
            1
                + u.members.reduce(
                    (acc, m) -> {
                        max(acc, depth(m))
                    },
                    0,
                )
        },
        let a: Array => 1 + depth(a.element),
        let m: Map => 1 + max(depth(m.key), depth(m.value)),
        let f: Fn => {
            1
                + max(
                    f.params.reduce(
                        (acc, p) -> {
                            max(acc, depth(param_ty(p)))
                        },
                        0,
                    ),
                    max(depth(f.ret), depth(f.error)),
                )
        },
        let c: Class => {
            1
                + c.args.reduce(
                    (acc, a) -> {
                        max(acc, depth(a))
                    },
                    0,
                )
        },
        Enum => 1,
        let i: Iface => {
            1
                + max(
                    i.args.reduce(
                        (acc, a) -> {
                            max(acc, depth(a))
                        },
                        0,
                    ),
                    i.pins.reduce(
                        (acc, p) -> {
                            max(acc, depth(p.ty))
                        },
                        0,
                    ),
                )
        },
        Alias => 1,
        Var => 1,
    }
}

/// The widest union anywhere in the type, or zero without one.
function union_width(ty: Ty) -> int throws never {
    match (ty) {
        Prim => 0,
        Never => 0,
        Top => 0,
        Lit => 0,
        EnumLit => 0,
        let u: Union => {
            max(
                u.members.length(),
                u.members.reduce(
                    (acc, m) -> {
                        max(acc, union_width(m))
                    },
                    0,
                ),
            )
        },
        let a: Array => union_width(a.element),
        let m: Map => max(union_width(m.key), union_width(m.value)),
        let f: Fn => {
            max(
                f.params.reduce(
                    (acc, p) -> {
                        max(acc, union_width(param_ty(p)))
                    },
                    0,
                ),
                max(union_width(f.ret), union_width(f.error)),
            )
        },
        let c: Class => {
            c.args.reduce(
                (acc, a) -> {
                    max(acc, union_width(a))
                },
                0,
            )
        },
        Enum => 0,
        let i: Iface => {
            max(
                i.args.reduce(
                    (acc, a) -> {
                        max(acc, union_width(a))
                    },
                    0,
                ),
                i.pins.reduce(
                    (acc, p) -> {
                        max(acc, union_width(p.ty))
                    },
                    0,
                ),
            )
        },
        Alias => 0,
        Var => 0,
    }
}

function max(a: int, b: int) -> int throws never {
    if (a > b) {
        a
    } else {
        b
    }
}
