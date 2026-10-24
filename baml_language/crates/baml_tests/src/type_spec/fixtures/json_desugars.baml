// R6 (S15 residue): to_json / from_json interface calls
//
// `recv.to_json()` calls the `baml.ToJson` interface method (works on ANY
// receiver: one with no explicit impl gets the structural default) and
// `Type.from_json(j)` calls the `baml.FromJson` interface method; on a
// generic `T` receiver `from_json` is the synthesized
// `(j: json) -> T throws baml.json.DecodeError`. Throws charge through the
// real stdlib signatures.
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
