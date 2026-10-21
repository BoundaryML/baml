// suffix and nested patterns after a rest slot take the element type
class SrpWrap {
    inner: int[]
}
function srp_suffix(p: SrpWrap | int[]) -> int throws never {
    let r = match (p) {
     // ^ int
        SrpWrap { inner: [.., let v: int, _] } | [_, let v: int, ..] => v,
        _ => 0
    }
    r
}
function srp_nested(v: int[][]) -> int throws never {
    let r = match (v) {
     // ^ int
        [.., [_, let x, ..]] => x,
        _ => 0
    }
    r
}
