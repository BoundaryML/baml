// an alias-typed subject still discharges Implements bounds (regression; negative test)
interface AbvShow {
    function show(self) -> string throws never
}
class AbvSilent {
    n: int
}
type AbvAlias = AbvSilent
function abv_take<T extends AbvShow>(v: T) -> int throws never {
    0
}
function abv_main(u: AbvAlias) -> int throws never {
    let r = abv_take(u)
         // ^^^^^^^^^^^ mismatch: expected user.AbvShow, got user.AbvAlias
    r
}
