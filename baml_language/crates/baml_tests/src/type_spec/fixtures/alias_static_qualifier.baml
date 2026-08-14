// alias qualifiers behave as their targets in static position
class AqBox {
    y: int

    implements baml.FromJson {
        function from_json(j: baml.json.json) -> Self throws baml.json.JsonDecodeError {
            AqBox { y: 99 }
        }
    }
}
type AqInner = AqBox
type AqOuter = AqInner
function aq_from_json() -> int throws baml.json.JsonParseError | baml.json.JsonDecodeError {
    let j = baml.json.parse("{}")
    let b = AqOuter.from_json(j)
     // ^ user.AqBox
    b.y
}
class AqMake {
    n: int

    function make(seed: int) -> AqMake throws never {
        AqMake { n: seed }
    }
}
type AqMaker = AqMake
function aq_static() -> int throws never {
    let m = AqMaker.make(7)
     // ^ user.AqMake
    m.n
}
