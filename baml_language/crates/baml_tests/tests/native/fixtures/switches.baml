// Native backend fixture: match arms that share the default's target.

function sw_empty(x: int) -> int {
    let y = 1;
    match (x) {
        1 => { y = y; }
        2 => { y = 5; }
        _ => { y = y; }
    }
    y
}
function sw_same(x: int) -> int {
    match (x) {
        1 => 7,
        2 => 7,
        _ => 7,
    }
}
function sw_fall(x: int) -> int {
    let t = 0;
    match (x) {
        1 => {}
        _ => {}
    }
    t + x
}
function chain(x: int) -> int {
    let t = 0;
    match (x) {
        0 => {}
        _ => {}
    }
    t = t + 0;
    match (x) {
        1 => {}
        _ => {}
    }
    t = t + 1;
    match (x) {
        2 => {}
        _ => {}
    }
    t = t + 2;
    match (x) {
        3 => {}
        _ => {}
    }
    t = t + 3;
    match (x) {
        4 => {}
        _ => {}
    }
    t = t + 4;
    match (x) {
        5 => {}
        _ => {}
    }
    t = t + 5;
    match (x) {
        6 => {}
        _ => {}
    }
    t = t + 6;
    match (x) {
        7 => {}
        _ => {}
    }
    t = t + 7;
    t
}
function partial(x: int) -> int {
    let t = 0;
    match (x) {
        1 => {}
        2 => { return 3; }
        _ => {}
    }
    t + x
}
