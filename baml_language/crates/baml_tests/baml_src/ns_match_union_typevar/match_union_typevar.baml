function tag_or_value<T>(x: T | string | null) -> T? {
    match (x) {
        let s: string => null,
        null => null,
        let v: T => v,
    }
}

function typevar_first<T>(x: T | string | null) -> T? {
    match (x) {
        let v: T => v,
        let s: string => null,
        null => null,
    }
}

class Sentinel {
}

function typevar_with_class_sibling<T>(x: T | Sentinel) -> T? {
    let y: T | Sentinel = x;
    match (y) {
        Sentinel {} => null,
        let v: T => v,
    }
}

test "tag-or-value union TypeVar match compiles" {
    assert.equal(tag_or_value<int>(7), 7)
}

test "TypeVar arm first leaves concrete arms reachable" {
    assert.equal(typevar_first<int>("hello"), null)
}

test "TypeVar union with a class sibling compiles" {
    assert.equal(typevar_with_class_sibling<int>(9), 9)
}
