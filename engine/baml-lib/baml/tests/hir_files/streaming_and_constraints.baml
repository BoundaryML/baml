class MyClass {
  field1 string @stream.done
  field2 int @stream.not_null
  field3 bool @stream.with_state
}

class User {
  name string @assert({{ this.length > 0 }})
  age int @check(valid_age, {{ this >= 0 }})
}

<<<<<<< HEAD
// class baml.HttpRequest {
//   url: string
//   method: baml.HttpMethod
//   headers: (map<string, string> | null)
//   query_params: (map<string, string> | null)
//   json: (ANY | null)
// }
//
=======
>>>>>>> canary
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
<<<<<<< HEAD
//
// enum baml.HttpMethod {
//   Get
//   Post
//   Put
//   Delete
//   Patch
// }
=======
>>>>>>> canary

// error: Error validating: This line is invalid. It does not start with any known Baml schema keyword.
//   -->  test.baml:12
//    | 
// 11 | 
// 12 | <<<<<<< HEAD
// 13 | // class baml.HttpRequest {
//    | 
