// Native backend fixture: match on integer and boolean literals.

function classify(x: int) -> int {
    match (x) {
        0 => 100,
        1 => 200,
        2 => 300,
        _ => x * 10,
    }
}

function match_returns(x: int) -> int {
    let v = match (x) {
        0 => return -1,
        _ => x,
    };
    v + 100
}

function match_loop(n: int) -> int {
    let total = 0;
    let i = 0;
    while (i < n) {
        i = i + 1;
        match (i) {
            6 => break,
            3 => continue,
            _ => { total = total + i; }
        }
    }
    total
}

function bool_match(b: bool) -> int {
    match (b) {
        true => 1,
        false => 0,
    }
}
