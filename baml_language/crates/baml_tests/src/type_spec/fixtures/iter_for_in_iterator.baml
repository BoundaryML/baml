// for-in over an iterator value forces the element at the structure demand
function ifi_array_iter() -> int[] throws never {
    let out: int[] = []
    for (let x in baml.iter.ArrayIterator.new([2, 4, 6])) {
        let element = x
         // ^^^^^^^ int
        out.push(element)
    }
    out
}
function ifi_range() -> int[] throws never {
    let out: int[] = []
    for (let x in baml.iter.Range.new(1, 4)) {
        let element = x
         // ^^^^^^^ int
        out.push(element)
    }
    out
}
