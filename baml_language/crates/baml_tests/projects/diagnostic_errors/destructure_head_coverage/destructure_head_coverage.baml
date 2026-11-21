// A destructure head is resolved as a constructor head is: an alias of a
// class (directly, through a chain, or pinning a generic class's args)
// destructures that class, and its witnesses name the class. A head that
// names no class or interface is reported, and its arm covers nothing
// beyond that report.
class PbFoo {
    v: int,
}

class PbOther {
    v: int,
}

class PbBox<T> {
    value: T,
}

interface PbHasV {
    v: int,
}

type PbAlias = PbFoo;

type PbChain = PbAlias;

type PbIntBox = PbBox<int>;

type PbUnion = PbFoo | PbOther;

type PbEnumAlias = PbE;

type PbIfaceAlias = PbHasV;

enum PbE {
    A,
}

function alias_missing_null(x: PbFoo | null) -> int {
    match (x) {
        PbAlias { v: _ } => 1,
    }
}

function chain_missing_value(x: PbFoo) -> int {
    match (x) {
        PbChain { v: 1 } => 1,
    }
}

function generic_target_missing_value(b: PbBox<int>) -> int {
    match (b) {
        PbIntBox { value: 1 } => 1,
    }
}

function union_alias_head(x: PbFoo | PbOther) -> int {
    match (x) {
        PbUnion { v: 1 } => 1,
    }
}

function enum_alias_head(x: PbFoo) -> int {
    match (x) {
        PbEnumAlias {} => 1,
    }
}

function interface_alias_head(x: PbFoo) -> int {
    match (x) {
        PbIfaceAlias { v: _ } => 1,
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
