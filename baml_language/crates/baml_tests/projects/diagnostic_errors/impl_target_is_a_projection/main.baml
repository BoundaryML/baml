// An impl names its interface by path. With a class `baml` in this namespace,
// `baml.ToString` starts at that class — the package is unreachable by that
// name from here — and a projection off a class names no interface. The
// header is refused; the compiler must not ask the class's impls what the
// path means (that question includes this very impl).

class baml {
    zz_field int
}

class Widget {
    zq int
}

implements baml.ToString for Widget {
    function to_string(self) -> string throws never {
        "x"
    }
}
