// Enum variant as a VALUE
//
// `Shape.Rectangle` in value position has the variant's singleton
// literal type - the same product the type-position path gives
// (r-a: value-namespace path resolution to an enum variant).
enum EvShape {
    Circle
    Rectangle
}

function pick() -> EvShape throws never {
    let s = EvShape.Rectangle;
     // ^ user.EvShape.Rectangle
    s
}

function flip(v: EvShape) -> EvShape throws never {
    match (v) {
        EvShape.Circle => EvShape.Rectangle,
        EvShape.Rectangle => EvShape.Circle,
    }
}
