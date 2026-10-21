// Every report inference makes from an implements goal records on the
// `implements` channel, so a fixture that meets one has to state it. Beside
// a plain "does not implement", a goal is reported as a `for` over a value
// that is not iterable, as an abstract argument where a bound needs a
// concrete type, and as a goal more than one implementation could prove.
interface Show {
    function show(self) -> string throws never
}

class Cat {
    implements Show {
        function show(self) -> string throws never { "cat" }
    }
}

class Dog {
    implements Show {
        function show(self) -> string throws never { "dog" }
    }
}

function show_it<T extends Show>(t: T) -> string throws never {
    t.show()
}

function not_iterable() -> int throws never {
    let total = 0;
    for (let x in 5) {
 //               ^ implements: 5 is not iterable
        total += 1;
    }
    total
 // ^^^^^ int
}

function not_concrete(pet: Cat | Dog) -> string throws never {
    show_it(pet)
//  ^^^^^^^^^^^^ implements: user.Cat | user.Dog is not concrete, as user.Show requires
}

interface Conv<A> {
    function to(self) -> A throws never
}

class W {
    v int
}

implements Conv<int> for W {
    function to(self) -> int throws never { self.v }
}

implements Conv<string> for W {
    function to(self) -> string throws never { "w" }
}

function needs_conv<A, T extends Conv<A>>(t: T) -> int throws never { 0 }

function ambiguous(w: W) -> int throws never {
    needs_conv(w)
//  ^^^^^^^^^^^^^ implements: user.W implements user.Conv<!error> more than one way
}
