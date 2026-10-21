// I3: interface FIELDS type through an existential receiver.
interface Named {
    name string
}

function f(n: Named) -> string {
    n.name
 // ^^^^^^ string
}
