// === Array.map with throwing callback ===

// Pure map callback
function test_array_map_pure() -> int[] {
  let items: int[] = [1, 2, 3]
  items.map((x) -> { x * 2 })
}

// Throwing map callback
function test_array_map_throwing() -> int[] {
  let items: int[] = [1, 2, 3]
  items.map((x) -> {
    if (x == 2) { throw "found two" }
    x * 2
  })
}
