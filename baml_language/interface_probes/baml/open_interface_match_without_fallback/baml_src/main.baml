// Negative probe: enumerating all currently known implementations of an open
// interface must not make the match exhaustive.
interface OpenMarker {}

class KnownA {
    implements OpenMarker {}
}

class KnownB {
    implements OpenMarker {}
}

function invalid_exhaustive_match(value: OpenMarker) -> string throws never {
    match (value) {
        let a: KnownA => "a",
        let b: KnownB => "b",
    }
}
