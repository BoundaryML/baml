class Array<T> {
  function length(self) -> int {
    $rust_function
  }

  function at(self, index: int) -> T? {
    $rust_function
  }

  function push(self, item: T) -> int {
    $rust_function
  }

  function pop(self) -> T? {
    $rust_function
  }

  function concat(self, other: T[]) -> T[] {
    $rust_function
  }

  function reverse(self) -> T[] {
    $rust_function
  }

  function slice(self, start: int, end: int) -> T[] {
    $rust_function
  }

  function join(self, separator: string) -> string {
    $rust_function
  }
}

class Map<K, V> {
  function length(self) -> int {
    $rust_function
  }

  function has(self, key: K) -> bool {
    $rust_function
  }

  function keys(self) -> K[] {
    $rust_function
  }

  function values(self) -> V[] {
    $rust_function
  }

  function set(self, key: K, value: V) {
    $rust_function
  }

  function get(self, key: K) -> V? {
    $rust_function
  }
}
