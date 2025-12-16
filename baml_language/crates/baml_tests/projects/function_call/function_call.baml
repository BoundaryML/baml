function Bar() -> int {
  let x = Foo(1);
  x
}

function Foo(x: int) -> int {
  x + 1
}

function GreetBaz(n: string) -> string {
  let baz = Baz { name: n };
  baz.Greeting()
}

class Baz {
  name string

  function Greeting(self) -> string {
    "Hello, " + self.name + "!"
  }
}
