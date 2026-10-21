// existential args discharge Implements goals through the requires closure
function ice_chain() -> int[] {
    let a: baml.iter.Iterator<Item = int, Error = never> = baml.iter.ArrayIterator.new([1, 2])
    let b: baml.iter.Iterator<Item = int, Error = never> = baml.iter.ArrayIterator.new([3, 4])
    let c = a.chain(b).collect()
     // ^ int[]
    c
}
