function f(t: (int, string)?, ts: (int, string)[], us: (int,)[]) -> int {
    if let (let a, "x") = t {
        a
    } else if let (let b,) = (1,) {
        b
    } else {
        0
    }
    if const (const c, _) = t {
        c
    } else {
        0
    }
    while let (let d, _) = t {
        break;
    }
    while let (let e,) = (1,) {
        break;
    }
    let (let g, _) = t else {
        return 0;
    };
    let (0,) = (g,) else {
        return 1;
    };
    for (let (let h, let i) in ts) {
        h;
    }
    for (const (const j,) in us) {
        j;
    }
    g
}
