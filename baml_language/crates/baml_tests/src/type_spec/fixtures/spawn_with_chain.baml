// Spawn with-chain modifiers + labeled args by name
//
// A modifier is demanded through `Plan.with`, whose associated-type pins
// (`Output`, `Error`, `ApplyError`) are solved from the modifier's own impl --
// so the chain's type flows link to link without any subtype check against
// open slots. Labeled call arguments select their parameter by NAME:
// `Plan.new(name = ..., body = ...)` checks each against its own parameter,
// not whatever sits first.
function swc_work() -> int throws never {
    1
}

function chained() -> int throws never {
    let g = baml.spawn.Limit.new(2) catch (e) {
        baml.errors.InvalidArgument => baml.sys.panic("two is positive"),
    };
    let t = spawn with g { swc_work() };
     // ^ baml.future.Future<int, never>
    await t
}

function with_cancel(tok: baml.spawn.CancelToken) -> int throws never {
    let t = spawn with tok, baml.spawn.Root.new() { swc_work() };
     // ^ baml.future.Future<int, never>
    await t
}

function by_name() -> baml.spawn.Plan<int, never> throws never {
    let p = baml.spawn.Plan.new(name = "labeled", body = () -> int { swc_work() });
     // ^ baml.spawn.Plan<int, never>
    p
}
