// pending: array-pattern ascriptions resolve aliases (if-let peer rides along, latent)
type SppInts = int[]
function spp_ret() -> SppInts throws never {
    [1, 2]
}
function spp_ascribed() -> int throws never {
    let [a, b]: SppInts = spp_ret()
    let r = a + b
     // ^ int
    r
}
function spp_if_let(m: string | null) -> int throws never {
    if let s: string = m {
        s.length()
    } else {
        0
    }
}
