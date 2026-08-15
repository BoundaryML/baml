class Box<T> {
  value T

  function get(self) -> T {
    self.value
  }

  function replace<U>(self, value: U) -> Box<U> {
    Box<U> { value: value }
  }

  function new<V>(value: V) -> Box<V> {
    Box<V> { value: value }
  }
}

class Counter {
  value int

  function add(self, amount: int) -> int {
    self.value + amount
  }

  function new(value: int) -> Counter {
    Counter { value: value }
  }
}

class Pair<A, B> {
  first A
  second B

  function get_second(self) -> B {
    self.second
  }

  function replace_second<C>(self, value: C) -> Pair<A, C> {
    Pair<A, C> { first: self.first, second: value }
  }
}

class Label {
  text string
}

enum Flavor {
  Vanilla
  Chocolate
}

function identity<T>(value: T) -> T {
  value
}

function type_name<T>() -> string {
  type.of<T>().to_string()
}

function head<T>(values: T[]) -> T {
  values[0]
}

function maybe<T>(value: T?) -> T? {
  value
}

function optional_maybe<T>(value: T? = null) -> T? {
  value
}

function map_values<T>(values: map<string, T>) -> T[] {
  values.values()
}

function wrap<T>(value: T) -> Box<T> {
  Box<T> { value: value }
}

function unbox<T>(value: Box<T>) -> T {
  value.value
}

function local_collision<T>(
  baml_type_0: T,
  baml_binding_0: T,
  baml_function: T,
  baml_arguments: T,
  cancellation_token: T,
) -> T {
  baml_arguments
}
