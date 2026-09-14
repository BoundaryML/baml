// `unreflect` is contextual syntax in one exact position and an ordinary
// identifier everywhere else: here a user's own class name and a local
// binding.

class unreflect { value int }

function unreflect_of(value: int) -> unreflect throws never {
    unreflect { value: value }
}

test "unreflect is an ordinary class and binding name" {
    let unreflect = unreflect_of(3)
    assert.equal(unreflect.value, 3)
}
