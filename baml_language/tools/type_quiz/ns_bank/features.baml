// What the sampler scores a case by. Names are the contract with
// `weights.baml`; the engine only sees numbers.

/// The features of a case: how many distinct rules its explanation uses
/// (without the flow rule every flow item ends with), how many times its
/// derivation flips a subtype into a supertype, the structural size of the
/// types it renders, and the models it traps.
function features_of(
    trace: root.engine.Claim[],
    flips: int,
    tys: root.algebra.Ty[],
    traps: string[],
) -> map<string, float> throws never {
    let none: string[] = [];
    let own = trace
        .map((claim) -> {
            claim.rule.name
        })
        .filter((name) -> {
            !is_site_rule(name)
        })
        .reduce(
            (acc, name) -> {
                if (acc.includes(name)) {
                    acc
                } else {
                    acc.concat([name])
                }
            },
            none,
        );
    let size = tys.reduce(
        (acc, ty) -> {
            acc + root.algebra.size(ty)
        },
        0,
    );
    let depth = tys.reduce(
        (acc, ty) -> {
            acc.max(root.algebra.depth(ty))
        },
        0,
    );
    let width = tys.reduce(
        (acc, ty) -> {
            acc.max(root.algebra.union_width(ty))
        },
        0,
    );
    let features: map<string, float> = {
        "rules": own.length() * 1.0,
        "flips": flips * 1.0,
        "size": size * 1.0,
        "depth": depth * 1.0,
        "width": width * 1.0,
        "traps": traps.length() * 1.0,
        "boring": if (own.length() == 1 && traps.length() == 0) {
            1.0
        } else {
            0.0
        },
    };
    // A map literal cannot take a computed key, so the per-model flags are
    // set on the fresh map before it is returned.
    for (let model in traps) {
        features.set(`trap:${model}`, 1.0);
    }
    features
}
