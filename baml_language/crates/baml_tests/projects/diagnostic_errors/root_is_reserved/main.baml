// `root` is the path to the package itself (TYPE_SYSTEM.md § Declarations,
// Packages, and Names) and names nothing else: no item, binding, or type
// parameter may be called `root`, so `root.X` always means this package's `X`.
// Every form that declares a name is refused, at the name. The other item
// kinds named `root` live one per namespace (`ns_enum/`, `ns_interface/`,
// `ns_alias/`), so each reports once.

class root {
    x int
}

function root() -> int {
    1
}

function generic<root>(value: root) -> root {
    value
}

class Holder<root> {
    value root
}

interface Bound<root> {
    function pick(self) -> root throws never
}

interface Sig {
    function required<root>(self, value: root) -> int throws never
    function required_param(self, root: int) -> int throws never
}

class Impl {
    v int
    function method<root>(self, value: root) -> root {
        value
    }
}

implements<root> Bound<root> for Holder<root> {
    function pick(self) -> root throws never {
        self.value
    }
}

function parameter(root: string) -> string {
    root
}

function binding() -> int {
    let root = 1
    root
}

function lambda() -> int {
    let f = (root: int) => { root }
    f(1)
}

function loop_binding(items: int[]) -> int {
    for (let root in items) {
        return root
    }
    0
}

function caught() -> int {
    {
        1
    } catch (root) {
        _ => 2
    }
}

function matched(value: int | string) -> int {
    match (value) {
        let root: int => root,
        _ => 0
    }
}

function destructured(holder: Holder<int>) -> int {
    let Holder<int> { value: let root } = holder
    root
}

function array_pattern(items: int[]) -> int {
    match (items) {
        [let root, ..] => root,
        _ => 0
    }
}

function scoped_type() -> int {
    type root = int
    1
}
