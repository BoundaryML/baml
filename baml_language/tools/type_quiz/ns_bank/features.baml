// What the sampler scores a case by. Names are the contract with
// `weights.baml`; the engine only sees numbers.

function features_of(
    d: root.algebra.Derivation,
    source: root.algebra.Ty,
    target: root.algebra.Ty,
    traps: string[],
) -> map<string, float> throws never {
    let flow = flow_requires_subtype().name;
    let own: string[] = [];
    for (let step in d.steps) {
        let name = root.algebra.step_rule(step).name;
        if (name != flow && !own.includes(name)) {
            let _ = own.push(name);
        }
    }
    let features: map<string, float> = {};
    let _ = features.set("rules", own.length() * 1.0);
    let _ = features.set("flips", root.algebra.flips(d) * 1.0);
    let _ = features.set("size", (root.algebra.size(source) + root.algebra.size(target)) * 1.0);
    let _ = features.set(
        "depth",
        root.algebra.max(root.algebra.depth(source), root.algebra.depth(target)) * 1.0,
    );
    let _ = features.set(
        "width",
        root.algebra.max(root.algebra.union_width(source), root.algebra.union_width(target)) * 1.0,
    );
    let _ = features.set("traps", traps.length() * 1.0);
    for (let model in traps) {
        let _ = features.set(`trap:${model}`, 1.0);
    }
    let boring = own.length() == 1 && traps.length() == 0;
    let _ = features.set(
        "boring",
        if (boring) {
            1.0
        } else {
            0.0
        },
    );
    features
}
