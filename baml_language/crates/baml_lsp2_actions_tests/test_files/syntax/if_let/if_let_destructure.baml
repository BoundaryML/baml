// if-let with class destructuring binds field names in the then-branch.
class User {
  name string
  age int
}

// Plain destructure on a non-union scrutinee is irrefutable, so this should
// warn — `else` can never fire.
function DestructureIrrefutable(u: User) -> string {
  if let User { name, age } = u {
    name
  } else {
    "no name"
  }
}

class Admin {
  name string
}

// Refutable destructure: scrutinee is a union, pattern picks one variant.
function DestructureRefutable(u: User | Admin) -> string {
  if let User { name, age } = u {
    name
  } else {
    "admin"
  }
}

//----
//- diagnostics
// <no-diagnostics-expected>
