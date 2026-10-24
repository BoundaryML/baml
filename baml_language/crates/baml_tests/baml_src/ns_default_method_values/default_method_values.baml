// `default.<method>` as a VALUE inside an `implements` block: the interface's
// default body bound to `self`, over the frame an adopted default runs in -
// `[Self, interface args..]` - and never the override. The same target
// `default.<method>(..)` calls, carried as a value.

interface DmvGreeter {
    function greet(self) -> string throws never { "hello" }
    function go(self) -> string throws never
}

class DmvG {
    implements DmvGreeter {
        function greet(self) -> string throws never { "over" }
        function go(self) -> string throws never {
            let f = default.greet;
            f()
        }
    }
}

test "a default method value bypasses the override" {
    assert.equal(DmvG {}.go(), "hello");
    assert.equal(DmvG {}.greet(), "over")
}

// A generic class: the frame is `[Self = DmvBox<T>]`, which the receiver's
// own class arguments `[T]` cannot express.
interface DmvShower {
    function show(self) -> string throws never { reflect.Type.of<Self>().to_string() }
    function run(self) -> string throws never
}

class DmvBox<T> {
    v T
    implements DmvShower {
        function show(self) -> string throws never { "override" }
        function run(self) -> string throws never {
            let f = default.show;
            f()
        }
    }
}

test "a default method value on a generic class sees Self" {
    assert.equal(DmvBox { v: 1 }.run(), reflect.Type.of<DmvBox<int>>().to_string());
    assert.equal(DmvBox { v: "s" }.run(), reflect.Type.of<DmvBox<string>>().to_string())
}

// A generic interface: the frame is `[Self, U]`, and the value travels.
interface DmvMaker<U> {
    function make(self) -> U throws never
    function made(self) -> U throws never { self.make() }
    function via(self) -> U throws never
}

class DmvIntMaker {
    implements DmvMaker<int> {
        function make(self) -> int throws never { 7 }
        function made(self) -> int throws never { 0 }
        function via(self) -> int throws never {
            let f = default.made;
            dmv_apply(f)
        }
    }
}

function dmv_apply(f: () -> int throws never) -> int {
    f()
}

test "a default method value on a generic interface carries its arguments" {
    assert.equal(DmvIntMaker {}.via(), 7)
}

// A generic default method reached through `default`: its own type arguments,
// written or inferred, follow the adopted default's `[Self, interface args..]`
// frame, called or as a value.
interface DmvPairer<K> {
    function pair<V>(self, key: K, value: V) -> string throws never {
        reflect.Type.of<Self>().to_string() + " " + reflect.Type.of<K>().to_string() + " " + reflect.Type.of<V>().to_string()
    }
    function inferred(self, key: K) -> string throws never
    function written(self, key: K) -> string throws never
    function as_value(self, key: K) -> string throws never
}

class DmvKeyed<T> {
    v T
    implements DmvPairer<string> {
        function pair<V>(self, key: string, value: V) -> string throws never { "override" }
        function inferred(self, key: string) -> string throws never {
            default.pair(key, true)
        }
        function written(self, key: string) -> string throws never {
            default.pair<float>(key, 1.5)
        }
        function as_value(self, key: string) -> string throws never {
            let f: (string, int) -> string throws never = default.pair;
            f(key, 1)
        }
    }
}

function dmv_expected<S, V>() -> string {
    reflect.Type.of<S>().to_string() + " " + reflect.Type.of<string>().to_string() + " " + reflect.Type.of<V>().to_string()
}

test "a generic default method through default gets its own type arguments" {
    let k = DmvKeyed { v: 1 };
    assert.equal(k.inferred("k"), dmv_expected<DmvKeyed<int>, bool>());
    assert.equal(k.written("k"), dmv_expected<DmvKeyed<int>, float>());
    assert.equal(k.as_value("k"), dmv_expected<DmvKeyed<int>, int>())
}
