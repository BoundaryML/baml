// Native backend fixture: `to_string` honoring a class's own `baml.ToString`
// at every depth, as the VM's three-pass walk does: the override-bearing
// objects are collected pre-order, each override is called in that order
// (so a mutating override is seen by the render and by the caller), and the
// structural rendering splices the results in by object identity.

class Tag {
    name: string,
    seen: int[],
    implements baml.ToString {
        function to_string(self) -> string throws never {
            self.seen.push(self.seen.length());
            "<" + self.name + ">"
        }
    }
}

// An empty block adopts the default and renders structurally.
class Plain {
    n: int,
    implements baml.ToString {}
}

class Holder {
    tags: Tag[],
    plain: Plain,
    label: string,
    by_name: map<string, Tag>,
    maybe: Tag | null,
}

function top(n: int) -> string {
    let t = Tag { name: "t" + n.to_string(), seen: [] };
    t.to_string() + "|" + t.seen.length().to_string()
}

function plain(n: int) -> string {
    Plain { n: n }.to_string()
}

function nested(n: int) -> string {
    let a = Tag { name: "a", seen: [] };
    let b = Tag { name: "b", seen: [] };
    let h = Holder {
        tags: [a, b, a],
        plain: Plain { n: n },
        label: "h",
        by_name: { "x": b },
        maybe: if (n > 0) { a } else { null },
    };
    h.to_string() + "|" + a.seen.length().to_string() + "|" + b.seen.length().to_string()
}

function in_array(n: int) -> string {
    let a = Tag { name: "a", seen: [] };
    [a, a].to_string() + "|" + a.seen.to_string()
}

function deep_override(n: int) -> int {
    if (n == 0) { [Tag { name: "d", seen: [] }].to_string().length() } else { deep_override(n - 1) + 1 }
}
