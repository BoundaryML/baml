// ── Class Field Access ──────────────────────────────────────────

function ReadField(p: Point) -> int {
    return p.x;
}

// bad field access → error
function BadField(p: Point) -> int {
    return p.z;
}

// ── Nested Class Access ─────────────────────────────────────────

function UseContainer(c: Container) -> int {
    return c.items[0];
}

// ── Assert ──────────────────────────────────────────────────────

function WithAssert(x: int) -> int {
    assert x > 0;
    return x;
}
