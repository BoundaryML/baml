// Header comments (`//#`) are statement-level nodes for the visualizer.
// They are only valid in expression contexts, including function bodies
// and match arms.

interface Sound {
    // Ordinary comments remain valid before interface members.
    function sound(self) -> string throws never
}

class Dog {
    // Ordinary comments remain valid before fields.
    name string
    // Ordinary comments remain valid before implements blocks.
    implements Sound {
        // Ordinary comments remain valid before implementation members.
        function sound(self) -> string {
            //# statement position
            "woof"
        }
    }
}

function classify(n: int) -> string {
    match (n) {
        //# leading header before the first arm
        0 => "zero",
        //# header between arms
        1 | 2 | 3 => "small",
        _ => "big",
    }
}

test "header comments parse in expression positions" {
    let d = Dog { name: "rex" };
    assert.equal(d.sound(), "woof");
    assert.equal(classify(0), "zero");
    assert.equal(classify(2), "small");
    assert.equal(classify(9), "big")
}
