// assignment narrows the flow overlay to the assigned literal
// (B-618; TS's assignment narrowing). Intentionally ahead of TIR:
// TIR narrows the USE but checks returns against the declared type,
// so the checked-position case fails there.
function anl_checked() -> 3 throws never {
    let x = 1
    x = 3
    x
}
function anl_widens() -> int throws never {
    let x = 1
    x = 3
    let y = x
     // ^ int
    y
}
