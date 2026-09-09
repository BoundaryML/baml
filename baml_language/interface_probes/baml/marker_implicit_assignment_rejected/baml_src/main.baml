// Negative probe: an empty shape does not acquire a marker witness.
interface Marker {}

class SameShapeWithoutImpl {
}

function invalid_assignment() -> Marker throws never {
    let marker: Marker = SameShapeWithoutImpl {  };
    marker
}

