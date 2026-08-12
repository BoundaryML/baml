//
// (TIR terminates too - the fix is shared - but renders the union in
// written order, `Foo<T | int>`, vs the canonical sort.)
//
// B-1091 (https://linear.app/boundaryml2/issue/B-1091): the confirmed
// stack overflow - a self-referential bound (`T extends Foo<T | int>`)
// re-entering canonicalization from the type-var subtype arm with a
// fresh assumption set. Fixed by threading the co-inductive assumptions
// through the union algebra (`canonical_with`); the fix is in the
// SHARED algebra, so both engines terminate. This fixture existing and
// compiling IS the regression test.
interface Foo<T> {}

function f<T extends Foo<T | int>>(x: T) -> T throws never {
    let z: Foo<T | int> = x;
     // ^ user.Foo<int | T>
    x
}
