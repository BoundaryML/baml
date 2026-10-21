// A root-level item and a namespace of one name is an error: `gadgets.mk()`
// reads a member of the class, `root.gadgets.mk()` an item of the namespace,
// and a reader cannot tell which was meant from the shorter spelling.

class gadgets {
    function mk() -> string {
        "class"
    }
}

function bare() -> string {
    gadgets.mk()
}

function qualified() -> string {
    root.gadgets.mk()
}
