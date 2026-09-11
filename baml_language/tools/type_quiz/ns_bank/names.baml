// Cosmetic name pools. Names never change a verdict; they exist so a learner
// cannot recognise a case by its identifiers. The conformance suite compiles
// every pool entry in each position it is used in, so a keyword or a stdlib
// collision cannot enter a pool unnoticed.

function type_names() -> string[] throws never {
    ["Widget", "Ticket", "Parcel", "Beacon", "Ledger", "Orbit", "Kettle", "Harbor"]
}

function function_names() -> string[] throws never {
    ["shuffle", "settle", "measure", "assemble", "route", "harvest", "tally", "polish"]
}

/// Names for a generated type parameter. `T` is left out: the bank's generic
/// box declares its own `T`, and a case that binds one variable inside
/// another's declaration reads as though the two were related.
function param_names() -> string[] throws never {
    ["S", "U", "V", "W"]
}

function binding_names() -> string[] throws never {
    [
        "value",
        "item",
        "count",
        "total",
        "left",
        "right",
        "inner",
        "outer",
        "first",
        "last",
        "found",
        "given",
    ]
}

/// Names for a flow probe, avoiding `reserved` (the declarations the probe
/// already contains).
function site_names(
    seed: root.engine.Seed,
    reserved: string[],
) -> root.algebra.SiteNames throws never {
    let fns = root.engine.pick_distinct(seed, function_names(), 2);
    let bindings = root.engine.pick_distinct(fns.next, binding_names(), 3);
    let holders = root.engine.pick_distinct(
        bindings.next,
        type_names().filter((n) -> {
            !reserved.includes(n)
        }),
        2,
    );
    root.algebra.SiteNames {
        function_name: fns.value[0],
        sink: fns.value[1],
        value: bindings.value[0],
        target: bindings.value[1],
        instance: bindings.value[2],
        holder: holders.value[0],
        contract: holders.value[1],
    }
}
