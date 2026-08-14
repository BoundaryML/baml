// a rigid literal witness anchors the join (no fresh upgrade)
function jrw_one() -> 1 throws never {
    1
}
function jrw_anchored(c: bool) -> int throws never {
    let x = if c { 1 } else { jrw_one() }
     // ^ 1
    x
}
function jrw_all_fresh(c: bool) -> int throws never {
    let y = if c { 1 } else { 1 }
     // ^ int
    y
}
