// pending: S6 let-annotation inference holes
// TYPE_SYSTEM.md (Interfaces section): `_` in a let binding annotation is a
// real inference hole filled exactly from the initializer.
function f() -> int {
    let x: _ = 1;
     // ^ int
    x
 // ^ int
}
