// `.as<T>` upcasts (Rust's qualified trait path)
//
// `x.as<UvParent<int>>` is `<X as Parent<i32>>` - the value viewed
// through a WRITTEN supertype. The base proves the view (implements
// oracle; var-carrying targets register an anchored Implements
// obligation) and the result IS the target, so members flow through
// the existential road with exactly the requested args - which is what
// disambiguates same-name members across instantiations. Targets are
// INTERFACE views only (TIR's rule; a general widening is just an
// annotation).
interface UvParent<T> {
    function describe(self) -> string throws never
}

class UvBoth {
    implements UvParent<int> {
        function describe(self) -> string throws never {
            "int"
        }
    }

    implements UvParent<string> {
        function describe(self) -> string throws never {
            "string"
        }
    }
}

function uv_pick(b: UvBoth) -> string throws never {
    let v = b.as<UvParent<int>>;
     // ^ user.UvParent<int>
    let s = b.as<UvParent<string>>.describe();
     // ^ string
    s
}

// A view whose pin is met by a blanket impl's PROJECTION binding
// (`type Output = T.Item` reducing to `string` once `T` binds) - the
// registry matcher normalizes pins by value, like confirmation.
interface UvSource {
    type Item

    function get(self) -> Self.Item throws never
}

class UvText {
    text string

    implements UvSource {
        type Item = string

        function get(self) -> string throws never {
            self.text
        }
    }
}

interface UvRender {
    type Output

    function render(self) -> Self.Output throws never
}

class UvBox<T extends UvSource<Item = string>> {
    inner T
}

implements<T extends UvSource<Item = string>> UvRender for UvBox<T> {
    type Output = T.Item

    function render(self) -> Self.Output throws never {
        self.inner.get()
    }
}

function uv_blanket(w: UvBox<UvText>) -> string throws never {
    let r = w.as<UvRender<Output = string>>.render();
     // ^ string
    r
}
