// Positive probe: matching an open interface needs a fallback, even when the
// source project currently knows only these two implementing classes.
interface OpenMarker {}

class KnownA {
    implements OpenMarker {}
}

class KnownB {
    implements OpenMarker {}
}

function classify_open(value: OpenMarker) -> string throws never {
    match (value) {
        let a: KnownA => "a",
        let b: KnownB => "b",
        _ => "future-or-foreign",
    }
}

class OpenMatchObservation {
    a: string,
    b: string,
}

function open_match_observation() -> OpenMatchObservation throws never {
    let a: OpenMarker = KnownA {  };
    let b: OpenMarker = KnownB {  };
    OpenMatchObservation { a: classify_open(a), b: classify_open(b) }
}

test "known implementations match before the required fallback" {
    let observed = open_match_observation();
    assert.equal(observed.a, "a");
    assert.equal(observed.b, "b")
}
