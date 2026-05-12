class MyLorem {
  a: int
}

function add_three_to_field_a(input_lorem: MyLorem) -> MyLorem {
  MyLorem { a: input_lorem.a + 3 }
}

function default_score(query: string, max_results: int = 10, filter: string? = null) -> int {
  if (filter == null) {
    max_results
  } else {
    max_results + 100
  }
}

function mutate_default(items: int[] = []) -> int {
  items.push(1)
  items.length()
}
