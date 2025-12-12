function MyFunction1(items: string[]) -> string {
  //# Loop
  watch let acc = "";

  for (let item in items) {
    acc = acc + item;
  }

  acc
}

function Main() -> string {
  MyFunction1(["a", "b", "c"])
}

test default {
  functions [Main]
  args { }
}
