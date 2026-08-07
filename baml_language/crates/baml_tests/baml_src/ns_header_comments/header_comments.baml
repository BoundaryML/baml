// Header comments (`//#`) are statement-level trivia for the visualizer.
// They must parse in every member/arm position, not just between block
// statements: class bodies, implements blocks, interface bodies, and
// match arms all accept them.

interface Sound {
    //# required members
    function sound(self) -> string throws never
}

class Dog {
    //# fields
    name string
    //# behavior
    implements Sound {
        //# interface conformance
        function sound(self) -> string {
            //# statement position (already worked)
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

test "header comments parse in class, implements, and match positions" {
    let d = Dog { name: "rex" };
    assert.equal(d.sound(), "woof");
    assert.equal(classify(0), "zero");
    assert.equal(classify(2), "small");
    assert.equal(classify(9), "big")
}
