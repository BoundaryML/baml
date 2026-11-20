function f(t: ((int) -> int,)) -> int {
    (t.0)(3)
}

function g(v: Holder) -> int {
    (v.f)(3)
}
