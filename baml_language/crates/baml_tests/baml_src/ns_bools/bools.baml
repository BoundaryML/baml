class FixedBoolRng {
    draw: uint8array,

    implements baml.random.Rng {
        function random(self, bytes: int) -> uint8array {
            self.draw
        }
    }
}

test "bool_random" {
    assert.equal(bool.random(rng = FixedBoolRng { draw: b"\x00" }), false);
    assert.equal(bool.random(rng = FixedBoolRng { draw: b"\x80" }), true);
    let default = bool.random();
    assert.is_true(default || !default)
}
