// pending: S7 control-flow joins
function h(c: bool) -> int {
    let v = if c { 1 } else { 2 };
     // ^ int
    v
}
