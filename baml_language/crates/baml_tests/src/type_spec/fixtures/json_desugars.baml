// R6 (S15 residue): to_json / from_json desugars
//
// `recv.to_json()` is sugar for `baml.json.from(recv)` (works on ANY
// receiver, no ToJson impl needed) and `Type.from_json(j)` is sugar for
// `baml.json.to<Type>(j)`; on a generic `T` receiver `from_json` is the
// synthesized `(j: json) -> T throws baml.json.DecodeError`. Throws charge
// through the real stdlib signatures.
class Point {
    x int
    y int
}

function ser(p: Point) -> json throws baml.json.SerializationError {
    let j = p.to_json();
     // ^ baml.json.json
    j
}

function ser_prim(s: string) -> json throws baml.json.SerializationError {
    s.to_json()
}

function de(j: json) -> Point throws baml.json.DecodeError {
    let p = Point.from_json(j);
     // ^ user.Point
    p
}

function de_generic<T>(j: json) -> T throws baml.json.DecodeError {
    let t = T.from_json(j);
     // ^ T
    t
}

function ser_generic<T>(v: T) -> json throws baml.json.SerializationError {
    v.to_json()
}
