// `unreflect` as a function name, called with the very `unreflect(` shape
// the type-argument lookahead keys on: in expression position it is an
// ordinary call.

class Marker { value int }

function unreflect(value: int) -> Marker throws never {
    Marker { value: value }
}

test "unreflect is an ordinary function name" {
    assert.equal(unreflect(3).value, 3)
}
