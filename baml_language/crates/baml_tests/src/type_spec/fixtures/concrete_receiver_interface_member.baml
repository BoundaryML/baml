// A method declared inside an in-body `implements` block reaches a
// CONCRETE receiver through the class's own method list (the item tree
// flattens in-body impl methods onto the class) - no impls-for-receiver
// walk needed for this spelling; out-of-body impl methods join with the
// symbolic resolvers.
interface Speak {
    function speak(self) -> string throws never
}

class Dog {
    sound string
    implements Speak {
        function speak(self) -> string throws never {
            "woof"
        }
    }
}

function f(d: Dog) -> string {
    d.speak()
 // ^^^^^^^^^ string
}
