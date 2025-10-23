class MyClass {
  field1 string @stream.done
  field2 int @stream.not_null
  field3 bool @stream.with_state
}

class User {
  name string @assert({{ this.length > 0 }})
  age int @check(valid_age, {{ this >= 0 }})
}

// class MyClass {
//   field1: string @stream.done
//   field2: int @stream.needed
//   field3: bool @stream.with_state
// }
//
// class User {
//   name: string @constrained
//   age: int @constrained
// }
