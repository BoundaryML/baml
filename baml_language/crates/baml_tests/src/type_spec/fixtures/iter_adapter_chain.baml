// Adapter chains off a constructor-call receiver
//
// `ArrayIterator.new([1, 2, 3, 4])` yields `ArrayIterator<?T>` whose
// `?T` is only lower-bounded mid-walk; method probing must resolve the
// WHOLE receiver, not just its head (rustc's
// resolve_vars_with_obligations before probing), so the impl registry
// - fail-safe against infer vars - sees `ArrayIterator<int>` and the
// interface adapters chain through.
function iac_chain() -> int[] throws never {
    let mapped = baml.iter.ArrayIterator.new([1, 2, 3, 4]).filter((x: int) -> bool {
     // ^^^^^^ int[]
        x % 2 == 0
    }).map((x: int) -> int {
        x * 10
    }).collect();
    mapped
}

function iac_reduce() -> int throws never {
    let total = baml.iter.ArrayIterator.new([1, 2, 3]).reduce((a: int, x: int) -> int {
     // ^^^^^ int
        a + x
    }, 0);
    total
}
