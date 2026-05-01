class Array<T> {
  function length(self) -> int {
    $rust_function
  }

  function at(self, index: int) -> T? {
    $rust_function
  }

  //baml:mut_self
  function push(self, item: T) -> int {
    $rust_function
  }

  //baml:mut_self
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

  //baml:vm
  function join(self, separator: string) -> string {
    $rust_function
  }

  //baml:mut_vm
  //baml:may_yield
  function map<U, E>(self, f: (T) -> U throws E) -> U[] throws E {
    $rust_function
  }
}

class Map<K, V> {
  function length(self) -> int {
    $rust_function
  }

  //baml:vm
  function has(self, key: K) -> bool {
    $rust_function
  }

  //baml:mut_vm
  function keys(self) -> K[] {
    $rust_function
  }

  function values(self) -> V[] {
    $rust_function
  }

  //baml:mut_self
  function set(self, key: K, value: V) -> null {
    $rust_function
  }

  //baml:vm
  function get(self, key: K) -> V? {
    $rust_function
  }
}
