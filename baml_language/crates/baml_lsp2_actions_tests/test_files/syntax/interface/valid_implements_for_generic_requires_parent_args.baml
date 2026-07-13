// Generic `requires` should carry instantiated parent args when checking
// out-of-body implements blocks. `Child<int>` requires `Parent<int>`, and the
// sibling `implements Parent<int> for int` provides the required method.

interface Parent<T> {
  function describe(self) -> string throws never
}

interface Child<T> requires Parent<T> {}

implements Parent<int> for int {
  function describe(self) -> string {
    return "parent-int"
  }
}

implements Child<int> for int {}

function use_child(child: Child<int>) -> string {
  return child.describe()
}

//----
//- diagnostics
// <no-diagnostics-expected>
