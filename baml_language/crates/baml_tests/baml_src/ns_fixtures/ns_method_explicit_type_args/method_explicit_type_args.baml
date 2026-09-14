class Foo {
  tag string
  function zero<T>(self) -> T? {
    null
  }
}

function use_method_type_args() -> int {
  let f = Foo { tag: "x" };
  let v: int = f.zero<int>() ?? 5;
  v
}
