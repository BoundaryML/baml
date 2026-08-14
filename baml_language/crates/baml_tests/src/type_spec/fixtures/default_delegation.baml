// `default.` delegation to interface default bodies
//
// Inside an `implements I` block, `default` is a scoped receiver (like
// `self`): `default.m()` calls I's DEFAULT implementation of `m` -
// Java's `I.super.m()` - with `Self` = the implementor. Resolved from
// the body owner's implements target, default-bodied members only.
interface DfShape {
    function label(self) -> string throws never {
        "shape"
    }

    function describe(self) -> string throws never
}

class DfBox {
    v int

    implements DfShape {
        function describe(self) -> string throws never {
            let base = default.label();
             // ^^^^ string
            base + "!"
        }
    }
}

// Same-name override wrapping its own default.
interface DfTag {
    function tag(self) -> string throws never {
        "tag"
    }
}

class DfWrap {
    implements DfTag {
        function tag(self) -> string throws never {
            let t = default.tag();
             // ^ string
            t + "!"
        }
    }
}

function df_use(b: DfBox, w: DfWrap) -> string throws never {
    b.describe() + w.tag()
}
