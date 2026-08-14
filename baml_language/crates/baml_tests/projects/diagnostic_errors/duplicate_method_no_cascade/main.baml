class Box {
    function value(self) -> int {
        1
    }

    function value(self) -> string {
        "hello"
    }
}

function repro(box: Box) -> int {
    box.value()
}
