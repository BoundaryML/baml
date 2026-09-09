// What the sampler scores a case by. Names are the contract with
// `weights.baml`; the engine only sees numbers.

function features_of(
    d: root.algebra.Derivation,
    source: root.algebra.Ty,
    target: root.algebra.Ty,
    traps: string[],
) -> map<string, float> throws never {
    let flow = flow_requires_subtype().name;
    let own = root.algebra.steps(d)
        .map((step) -> {
            root.algebra.step_rule(step).name
        })
        .filter((name) -> {
            name != flow
        })
        .reduce(
            (acc, name) -> {
                if (acc.includes(name)) {
                    acc
                } else {
                    acc.concat([name])
                }
            },
            [],
        );
    let features: map<string, float> = {
        "rules": own.length() * 1.0,
        "flips": root.algebra.flips(d) * 1.0,
        "size": (root.algebra.size(source) + root.algebra.size(target)) * 1.0,
        "depth": root.algebra.depth(source).max(root.algebra.depth(target)) * 1.0,
        "width": root.algebra.union_width(source).max(root.algebra.union_width(target)) * 1.0,
        "traps": traps.length() * 1.0,
        "boring": if (own.length() == 1 && traps.length() == 0) {
            1.0
        } else {
            0.0
        },
    };
    for (let model in traps) {
        features.set(`trap:${model}`, 1.0);
    }
    features
}
