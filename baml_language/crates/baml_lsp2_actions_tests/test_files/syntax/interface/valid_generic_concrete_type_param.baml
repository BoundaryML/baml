// BEP-044 §"Generic Interfaces": implementing a concrete instantiation of a
// generic interface should compile cleanly.

interface Container<T> {
  function add(self, item: T) -> null throws never
  function get(self, index: int) -> T? throws never
  function size(self) -> int throws never { return 0 }}

class IntStack {
  items: int[]

  implements Container<int> {
    function add(self, item: int) -> null {
      self.items.push(item)
      return null
    }
    function get(self, index: int) -> int? {
      return self.items[index]
    }
    function size(self) -> int {
      return self.items.length()
    }
  }
}

//----
//- diagnostics
// <no-diagnostics-expected>
