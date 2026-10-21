// I1: user-defined operator impls resolve through the same registry as
// the stdlib's - `implement baml.ops.Add<Meters> for Meters` makes
// `a + b` on Meters dispatch with Output read off the impl.
class Meters {
    value int
}

implement baml.ops.Add<Meters> for Meters {
    type Output = Meters;
    function add(self, rhs: Meters) -> Meters throws never {
        Meters { value: self.value + rhs.value }
    }
}

function f(a: Meters, b: Meters) -> Meters {
    a + b
 // ^^^^^ user.Meters
}
