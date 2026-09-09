// Positive probe: an empty interface remains nominal and opt-in.
interface Marker {}

class ExplicitMember {
    implements Marker {}
}

class SameShapeWithoutImpl {
}

class MarkerObservation {
    explicit_reflects: bool,
    same_shape_reflects: bool,
    explicit_matches: string,
    same_shape_matches: string,
}

function classify(value: unknown) -> string throws never {
    match (value) {
        let marker: Marker => "marker",
        _ => "other",
    }
}

function marker_observation() -> MarkerObservation throws never {
    let explicit = ExplicitMember {  };
    let same_shape = SameShapeWithoutImpl {  };
    MarkerObservation {
        explicit_reflects: reflect.Type.of<ExplicitMember>().implements(reflect.Type.of<Marker>()),
        same_shape_reflects: reflect.Type.of<SameShapeWithoutImpl>().implements(reflect.Type.of<Marker>()),
        explicit_matches: classify(explicit),
        same_shape_matches: classify(same_shape),
    }
}

test "empty interface membership is explicit" {
    let observed = marker_observation();
    assert.is_true(observed.explicit_reflects);
    assert.is_true(!observed.same_shape_reflects);
    assert.equal(observed.explicit_matches, "marker");
    assert.equal(observed.same_shape_matches, "other")
}
