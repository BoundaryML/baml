//
// B-919: https://linear.app/boundaryml2/issue/B-919
// A generic class destructure ADOPTS its type args from the scrutinee
// when exactly one same-class member determines them - the adoption TIR
// implements for interface heads but not classes, so it demands
// explicit `Pack<int> { ... }` here.
class Pack<T> {
    value T
}

function f() -> int {
    let boxed: Pack<int> = Pack { value: 42 };
    let Pack { value } = boxed;
    value
 // ^^^^^ int
}
