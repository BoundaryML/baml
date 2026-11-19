// A destructure whose head names no class or interface is reported, and
// its arm covers nothing beyond that report. Both were accepted silently,
// as if the arm were `_`.
class PbFoo {
    v: int,
}

type PbAlias = PbFoo;

enum PbE {
    A,
}

function alias_head(x: PbFoo) -> int {
    match (x) {
        PbAlias { v: 1 } => 1,
    }
}

function enum_head(x: PbFoo | int) -> int {
    match (x) {
        PbE {} => 1,
    }
}

// Reported elsewhere: an unresolved head, and a destructure under a field
// whose type failed to resolve. Neither stacks a coverage error.
class PbBad {
    v: PbMissing,
}

function errored_field(x: PbBad) -> int {
    match (x) {
        PbBad { v: PbFoo { v: 1 } } => 1,
    }
}

function unresolved_head(x: PbFoo | int) -> int {
    match (x) {
        PbNope { v: 1 } => 1,
    }
}
