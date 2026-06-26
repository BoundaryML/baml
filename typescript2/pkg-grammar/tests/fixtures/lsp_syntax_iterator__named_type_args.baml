function takes_strings(it: baml.iter.Iterator<Item = string, Error = never>) -> string[] throws never {
  it.collect()
}

function wrong_iterator_item() -> string[] throws never {
  takes_strings(baml.iter.ArrayIterator.new([1, 2, 3]))
}

function wrong_map_result() -> baml.iter.Iterator<Item = int, Error = never> throws never {
  baml.iter.ArrayIterator.new([1, 2]).map((x: int) -> string { "not an int" })
}

function wrong_flatten_item() -> baml.iter.Iterator<Item = int, Error = never> throws never {
  baml.iter.flatten(baml.iter.ArrayIterator.new([1, 2, 3]))
}

function wrong_error_binding() -> baml.iter.Iterator<Item = int, Error = string> throws never {
  baml.iter.ArrayIterator.new([1, 2, 3])
}
