// Shapes a consumer reaches through a source-less mount by identity: aliases
// (declared, used in exported shapes, recursive), runtime classes the consumer
// cannot fully spell, and a namespaced class's methods. Each was either a
// link-stub gap or a stub fall-open before the loader bound imports through
// the mounted package's own tables.

function scores() -> reflect.Package {
    reflect.Package.compile({ "lib.baml": `
        type Score = int
        class Board { best Score }
        function bump(s: Score) -> Score throws never { s + 1 }
        function board(best: Score) -> Board throws never { Board { best: best } }
    ` })
}

function consumer_of(files: map<string, string>, app: reflect.Package) -> reflect.Package {
    reflect.Package.compile(files, packages = { "app": app })
}

test "mounted alias merely declared is a spelling of its target" {
    let consumer = consumer_of(
        { "main.baml": `function go(s: app.Score) -> app.Score throws never { app.bump(s) }` },
        scores(),
    );
    let go = consumer.get_function<(int) -> int>("root.go") ?? throw "the consumer has no `go`";
    assert.equal(go(2), 3)
}

test "mounted alias used in exported shapes resolves in the consumer" {
    let consumer = consumer_of(
        { "main.baml": `
            function go() -> int throws never {
                let b: app.Board = app.board(app.bump(1));
                app.Board { best: b.best + app.bump(b.best) }.best
            }
        ` },
        scores(),
    );
    let go = consumer.get_function<() -> int>("root.go") ?? throw "the consumer has no `go`";
    assert.equal(go(), 5)
}

function trees() -> reflect.Package {
    reflect.Package.compile({ "lib.baml": `
        type Tree = int | Tree[]
        function leaf(n: int) -> Tree throws never { n }
        function wrap(t: Tree) -> Tree throws never { [t] }
        function count(t: Tree) -> int throws never {
            match (t) {
                let n: int => 1,
                let ts: Tree[] => {
                    let c = 0;
                    for (let x in ts) {
                        c = c + count(x);
                    }
                    c
                },
            }
        }
    ` })
}

test "recursive alias across the mount in parameter and return positions" {
    let consumer = consumer_of(
        { "main.baml": `
            function go() -> int throws never {
                let t: app.Tree = app.wrap([app.leaf(1), app.wrap(app.leaf(2))]);
                app.count(t) + app.count(app.leaf(7))
            }
        ` },
        trees(),
    );
    let go = consumer.get_function<() -> int>("root.go") ?? throw "the consumer has no `go`";
    assert.equal(go(), 3)
}

function empty_base() -> reflect.Package {
    reflect.Package.compile({ "lib.baml": `function noop() -> int throws never { 0 }` })
}

test "a mounted runtime class the consumer cannot name constructs a real instance" {
    let cls = reflect.class.new("bad-name", { "n": reflect.Type.of<int>() });
    let app = empty_base().with_types({ "Good": cls });
    let consumer = consumer_of(
        { "main.baml": `function go() -> int throws never { app.Good { n: 4 }.n }` },
        app,
    );
    let go = consumer.get_function<() -> int>("root.go") ?? throw "the consumer has no `go`";
    assert.equal(go(), 4)
}

test "a mounted runtime class with an unspellable optional field constructs a real instance" {
    let cls = reflect.class.new(
        "Opt",
        { "ok": reflect.Type.of<bool>(), "bad-field": reflect.Type.of<int?>() },
    );
    let app = empty_base().with_types({ "Opt": cls });
    let consumer = consumer_of(
        { "main.baml": `function go() -> bool throws never { app.Opt { ok: true }.ok }` },
        app,
    );
    let go = consumer.get_function<() -> bool>("root.go") ?? throw "the consumer has no `go`";
    assert.is_true(go())
}

function widgets() -> reflect.Package {
    reflect.Package.compile({ "ns_models/w.baml": `
        class W {
            n int
            function m(self) -> int throws never { self.n }
            function make(n: int) -> W throws never { W { n: n } }
        }
    ` })
}

test "a namespaced class's methods dispatch across the mount" {
    let consumer = consumer_of(
        { "main.baml": `
            function go() -> int throws never {
                app.models.W { n: 5 }.m() + app.models.W.make(3).m()
            }
        ` },
        widgets(),
    );
    let go = consumer.get_function<() -> int>("root.go") ?? throw "the consumer has no `go`";
    assert.equal(go(), 8)
}
