// Optional / nullable scrutinee: `if let x: T = optional_value` should
// bind `x: T` in the then-branch and narrow the scrutinee to `null` in
// the else-branch (when the scrutinee is a path local).

// Simple primitive optional.
function PrimitiveOptional(x: int?) -> int {
  if let v: int = x {
    v
  } else {
    // `x` narrows to `null` here.
    0
  }
}

// Class optional.
class Item {
  name string
}

function ClassOptional(item: Item?) -> string {
  if let i: Item = item {
    i.name
  } else {
    "none"
  }
}

// Optional with destructuring.
function DestructureOptional(item: Item?) -> string {
  if let Item { name } = item {
    name
  } else {
    "none"
  }
}

//----
//- diagnostics
// <no-diagnostics-expected>
