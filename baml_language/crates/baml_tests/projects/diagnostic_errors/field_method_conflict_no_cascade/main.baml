class Box {
    value: int,

    function value(self) -> string {
        "hello"
    }
}

function repro(box: Box) -> string {
    box.value()
}
