// every instantiation registers its declared bounds
interface RbShow {
    function show(self) -> string throws never
}
class RbPlain {
    n: int
}
class RbBox {
    function make<T extends RbShow>(v: T) -> int throws never {
        0
    }
}
class RbHolder {
    tag: string

    function pick<T extends RbShow>(self, v: T) -> int throws never {
        0
    }
}
function rb_static(p: RbPlain) -> int throws never {
    let r = RbBox.make(p)
         // ^^^^^^^^^^^^^ mismatch: expected user.RbShow, got user.RbPlain
    r
}
function rb_method_value(h: RbHolder, p: RbPlain) -> int throws never {
    let g = h.pick
    //      ^^^^^^ mismatch: expected user.RbShow, got user.RbPlain
    let r = g(p)
    r
}
