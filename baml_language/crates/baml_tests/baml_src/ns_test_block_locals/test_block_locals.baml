// Locals holding INSTANCES inside `test` blocks, directly — no helper
// functions. A test-block local-boxing VM bug once made this shape
// misbehave (the dispatch-matrix fixture and the Rust-to-corpus test
// conversions routed every body through top-level helpers to dodge it);
// it no longer reproduces, and this fixture pins that so the avoidance
// pattern can die instead of fossilizing.

interface TblSpeaker {
    function speak(self) -> string throws never {
        "default"
    }
}

class TblLoud {
    tag: string
    implements TblSpeaker {
        function speak(self) -> string throws never {
            "loud " + self.tag
        }
    }
}

class TblQuiet {
    implements TblSpeaker {}
}

test "test_block_local_instance_method_call" {
    let l = TblLoud { tag: "a" };
    assert.equal(l.speak(), "loud a")
}

test "test_block_local_instance_field_read" {
    let l = TblLoud { tag: "b" };
    assert.equal(l.tag, "b")
}

test "test_block_local_adopted_default" {
    let q = TblQuiet {};
    assert.equal(q.speak(), "default")
}

test "test_block_local_interface_typed_value" {
    let s: TblSpeaker = TblLoud { tag: "c" };
    assert.equal(s.speak(), "loud c")
}

test "test_block_local_bound_method" {
    let l = TblLoud { tag: "e" };
    let f = l.speak;
    assert.equal(f(), "loud e")
}

test "test_block_local_union_typed" {
    let u: TblLoud | TblQuiet = TblQuiet {};
    assert.equal(u.speak(), "default")
}

test "test_block_local_mutation" {
    let l = TblLoud { tag: "f" };
    l.tag = "g";
    assert.equal(l.speak(), "loud g")
}

test "test_block_local_interpolation" {
    let l = TblLoud { tag: "h" };
    assert.equal(`${l.speak()}`, "loud h")
}
