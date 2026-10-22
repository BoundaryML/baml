// A generic function used as a value carries the instantiation its context
// infers: called later, its body sees the type arguments it was specialized
// at, whatever road reaches it.

function wrap<T>(x: T) -> T[] {
    [x]
}

function wrap_tagged<T>(x: T, tag: string = "t") -> T[] {
    [x]
}

class Wrapper {
    function wrap<T>(self, x: T) -> T[] {
        [x]
    }

    function wrap_static<T>(x: T) -> T[] {
        [x]
    }
}

function apply<A, B>(x: A, f: (A) -> B) -> B {
    f(x)
}

test "a free generic function value through a stdlib higher-order function" {
    assert.equal([1, 2].map(wrap), [[1], [2]])
    assert.equal([1, 2].map(wrap_tagged), [[1], [2]])
}

test "a generic function value bound to an annotated local" {
    let f: (int) -> int[] throws never = wrap;
    assert.equal(f(1), [1])
    let g: (int) -> int[] throws never = wrap_tagged;
    assert.equal(g(2), [2])
}

test "a generic function value through a user higher-order function" {
    assert.equal(apply(1, wrap), [1])
}

test "generic method values, bound and static" {
    let w = Wrapper {};
    assert.equal([1, 2].map(w.wrap), [[1], [2]])
    assert.equal([1, 2].map(Wrapper.wrap_static), [[1], [2]])
}

function reflect_elem<T>(x: T) -> reflect.Type {
    reflect.Type.of<T>()
}

test "the value's type argument reaches reflection" {
    let f: (string) -> reflect.Type throws never = reflect_elem;
    assert.is_true(f("a") == reflect.Type.of<string>())
}
