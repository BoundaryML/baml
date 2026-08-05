// Interface-method OWN generics instantiate at the call site
// `iter.Iterator.map<R, E2>` / `reduce<A, E2>` are interface DEFAULT
// methods with their own generic params. The interface frame (`Self`,
// interface generics, assoc slots) comes from the receiver; the
// member's own params fill from the call site (turbofish or fresh
// vars) - not leak as rigid `R`/`A`/`E2` into the lambda checks.
function iter_map_collect() -> int[] throws never {
    let doubled = [1, 2, 3].iter().map((x: int) -> int { x * 2 }).collect();
     // ^^^^^^^ int[]
    doubled
}

function iter_reduce() -> int throws never {
    let it: baml.iter.Iterator<Item = int, Error = never> = baml.iter.ArrayIterator.new([1, 2, 3]);
    let total = it.reduce((a: int, x: int) -> int { a + x }, 0);
     // ^^^^^ int
    total
}
