// Spawn with-chain modifiers + labeled args by name
//
// A with-modifier is demanded STRUCTURALLY (a fn returning
// SpawnParams), its generics solved from the chain's current link -
// not subtype-checked against open `unknown` slots (class invariance
// would reject valid modifiers). Labeled call arguments select their
// parameter by NAME: `options(cancel = tok)` checks `tok` against
// `cancel`, not whatever parameter sits first.
function swc_work() -> int throws never {
    1
}

function chained() -> int throws never {
    let g = baml.spawn.TaskGroup.new(2);
    let t = spawn with baml.spawn.options(group = g) { swc_work() };
     // ^ Future<int, never>
    await t
}

function with_cancel(tok: baml.spawn.CancelToken) -> int throws never {
    let t = spawn with baml.spawn.options(cancel = tok, detach = true) { swc_work() };
     // ^ Future<int, never>
    await t
}
