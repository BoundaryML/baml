function apply(f: (x: int) -> int throws never, v: int) -> int {
    f(v)
}

function l() -> int {
    apply((x) -> { x }, 41)
      //           ^ int
}
