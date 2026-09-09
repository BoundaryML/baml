// Cosmetic name pools. Names never change a verdict; they exist so a learner
// cannot recognise a case by its identifiers. The conformance suite compiles
// every pool entry as an identifier of its kind, so a keyword or a stdlib
// collision cannot enter a pool unnoticed.

function type_names() -> string[] throws never {
    ["Widget", "Ticket", "Parcel", "Beacon", "Ledger", "Orbit", "Kettle", "Harbor"]
}

function function_names() -> string[] throws never {
    ["shuffle", "settle", "measure", "assemble", "route", "harvest", "tally", "polish"]
}

function binding_names() -> string[] throws never {
    ["a", "b", "c", "x", "y", "z", "value", "item", "count", "total", "left", "right"]
}

/// `n` distinct names from `pool` and the seed that continues the stream.
class Picked {
    names: string[],
    next: root.engine.Seed,
}

/// Pick `n` distinct names from `pool`. Yields fewer only if the pool is
/// smaller than `n`.
function pick_distinct(seed: root.engine.Seed, pool: string[], n: int) -> Picked throws never {
    let remaining = pool.slice(0, pool.length());
    let names: string[] = [];
    let s = seed;
    while (names.length() < n && remaining.length() > 0) {
        let d = root.engine.below(s, remaining.length());
        s = d.next;
        if let name: string = remaining.remove_at(d.value) {
            let _ = names.push(name);
        }
    }
    Picked { names: names, next: s }
}

/// Names for a flow probe, avoiding `reserved` (the declarations the probe
/// already contains).
function site_names(
    seed: root.engine.Seed,
    reserved: string[],
) -> root.algebra.SiteNames throws never {
    let fns = pick_distinct(seed, function_names(), 2);
    let bindings = pick_distinct(fns.next, binding_names(), 2);
    let holders = pick_distinct(
        bindings.next,
        type_names().filter((n) -> {
            !reserved.includes(n)
        }),
        1,
    );
    root.algebra.SiteNames {
        function_name: fns.names[0],
        sink: fns.names[1],
        value: bindings.names[0],
        target: bindings.names[1],
        holder: holders.names[0],
    }
}
