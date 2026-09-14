// Captured variables are heap cells, written by closures and by other tasks.
// The emitter re-evaluates a single-use local's defining expression at its use
// site ("virtualization") when nothing between definition and use can change
// the result; a cell read used to be treated like any other local read, so a
// value snapshotted BEFORE a write through the cell was re-read AFTER it
// (Linear B-1698). These pin the rule that a cell read never moves across a
// block boundary, in every shape that used to miscompile.

// A read inside a closure, then a write on one branch, then the earlier read.
function cells_read_then_branch_write() -> int {
    let x = 1;
    let f = () -> int {
        let before = x;
        if (x == 1) { x = 5 }
        before
    }
    f()
}

test "cells_read_then_branch_write" {
    assert.equal(cells_read_then_branch_write(), 1)
}

// A read in the outer frame, then a call to a closure that writes the cell.
function cells_read_then_call_writes() -> int {
    let x = 1;
    let g = () -> void { x = 5 }
    let before = x;
    g();
    before
}

test "cells_read_then_call_writes" {
    assert.equal(cells_read_then_call_writes(), 1)
}

// The same with a PARAMETER: a copy of a never-reassigned parameter is
// normally read straight from the parameter at its use sites, which is a
// second way to move the read past the write.
function cells_param_read_then_call_writes(x: int) -> int {
    let g = () -> void { x = 5 }
    let before = x;
    g();
    before
}

test "cells_param_read_then_call_writes" {
    assert.equal(cells_param_read_then_call_writes(1), 1)
}

// The write comes from a spawned task; the read must not move past the await.
function cells_read_then_spawn_writes() -> int {
    let x = 1;
    let f = spawn { x = 5; 0 };
    let before = x;
    let _ = await f;
    before
}

test "cells_read_then_spawn_writes" {
    assert.equal(cells_read_then_spawn_writes(), 1)
}

// The read happens before the spawned task even exists.
function cells_read_before_spawn_exists() -> int {
    let x = 1;
    let before = x;
    let f = spawn { x = 5; 0 };
    let _ = await f;
    before
}

test "cells_read_before_spawn_exists" {
    assert.equal(cells_read_before_spawn_exists(), 1)
}

// Wrapping the read in an array literal is not a workaround: the allocation
// is a single-use value too, and it moved with the read.
function cells_read_into_array_then_spawn_writes() -> int {
    let x = 1;
    let before = [x];
    let f = spawn { x = 5; 0 };
    let _ = await f;
    before[0]
}

test "cells_read_into_array_then_spawn_writes" {
    assert.equal(cells_read_into_array_then_spawn_writes(), 1)
}

class CellsSeen {
    v int
}

// Control: a spawned body does not run before the parent's next statement, so
// a read stored into a heap field between the spawn and the await sees the
// old value. Separates the codegen rule above from scheduling.
function cells_spawn_starts_at_first_yield() -> int {
    let seen = CellsSeen { v: 0 };
    let x = 1;
    let f = spawn { x = 5; 0 };
    seen.v = x;
    let _ = await f;
    seen.v
}

test "cells_spawn_starts_at_first_yield" {
    assert.equal(cells_spawn_starts_at_first_yield(), 1)
}
