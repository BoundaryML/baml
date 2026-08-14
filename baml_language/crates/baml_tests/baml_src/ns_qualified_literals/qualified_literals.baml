// Regression pin: fully-qualified struct literals inside array literals used
// to misparse ("expected expression, found ']'") while the same literal was
// fine at let/argument positions. The absolute-path form inside `[...]` is
// the shape that broke.

class QualPoint {
    x int
    y int
}

function first_x(points: root.qualified_literals.QualPoint[]) -> int {
    points.at(0)?.x ?? -1
}

test "qualified struct literals parse inside array literals" {
    let via_let = root.qualified_literals.QualPoint { x: 1, y: 2 };
    let arr = [root.qualified_literals.QualPoint { x: 3, y: 4 }, via_let];
    assert.equal(arr.length(), 2);
    assert.equal(first_x(arr), 3);
    assert.equal(first_x([root.qualified_literals.QualPoint { x: 7, y: 8 }]), 7)
}
