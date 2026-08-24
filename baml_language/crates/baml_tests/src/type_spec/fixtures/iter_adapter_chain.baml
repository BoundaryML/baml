// Adapter chains off a constructor-call receiver
//
// `ArrayIterator.new([1, 2, 3])` is `ArrayIterator<?T>` mid-walk (the
// element var solves at finish). Method resolution must PROBE impls by
// unification - candidates tried under a table snapshot, the unique
// applicable impl confirmed by linking the receiver's vars to the
// impl's fresh ones (rustc's probe/confirm; r-a's snapshot probes) -
// never by forcing the vars early. `flat_map` additionally solves its
// `R`/`E3` through the `Iterable<Item = R, Error = E3>` expectation
// (an Implements goal confirmed by the `T[]` impl).
function iac_chain() -> int[] throws never {
    let mapped = baml.iter.ArrayIterator.new([1, 2, 3, 4]).filter((x: int) -> bool {
     // ^^^^^^ int[]
        x % 2 == 0
    }).map((x: int) -> int {
        x * 10
    }).collect();
    mapped
}

function iac_flat() -> int[] throws never {
    let flattened = baml.iter.ArrayIterator.new([1, 2, 3]).flat_map((x: int) -> baml.iter.Iterable<Item = int, Error = never> {
     // ^^^^^^^^^ int[]
        [x, x + 10]
    }).collect();
    flattened
}
