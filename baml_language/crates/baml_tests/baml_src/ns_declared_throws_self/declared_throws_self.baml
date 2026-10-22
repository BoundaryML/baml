// A method's declared `throws` clause is read in its signature's
// environment, so `Self` there is the class, both for callers and for the
// method's own throw sites.

class DtsOops {
    code int

    function raise(self) -> int throws Self {
        throw self
    }
}

function dts_caught() -> int {
    let oops = DtsOops { code: 7 };
    oops.raise() catch (e) {
        let caught: DtsOops => caught.code
    }
}

test "a method throws the class its clause names as Self" {
    assert.equal(dts_caught(), 7)
}
