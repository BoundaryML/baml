class MyLorem {
  a: int
}

function add_three_to_field_a(input_lorem: MyLorem) -> MyLorem {
  MyLorem { a: input_lorem.a + 3 }
}
