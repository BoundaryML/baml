// array patterns claim the union member the pattern discriminates
function apc_ascribed(v: int[] | string[]) -> int throws never {
    let r = match (v) {
     // ^ int
        [let x: int] => v[0] + x,
        _ => 0
    }
    r
}
function apc_nested(p: int[][] | int[]) -> int throws never {
    let r = match (p) {
     // ^ int
        [_, [.., let v: int], ..] => v,
        _ => 0
    }
    r
}
