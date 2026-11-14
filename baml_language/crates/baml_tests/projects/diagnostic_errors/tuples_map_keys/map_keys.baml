// Tuples are not valid map keys, even when every element is hashable.

type ByPair = map<(int, int), string>

class Grid {
    cells map<(int, int), string>
}

function lookup(m: map<(string,), int>) -> int {
    0
}

function local_map() -> int {
    let m: map<(int, bool), int> = {};
    0
}

// A tuple does not satisfy a `Hash` bound.
function hash_of<T extends baml.Hash>(x: T) -> int {
    0
}

function hash_a_tuple() -> int {
    hash_of((1, 2))
}
