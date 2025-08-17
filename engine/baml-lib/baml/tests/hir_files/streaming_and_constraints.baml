class MyClass {
  field1 string @stream.done
  field2 int @stream.not_null
  field3 bool @stream.with_state
}

class User {
  name string @assert({{ this.length > 0 }})
  age int @check(valid_age, {{ this >= 0 }})
}

// class std::Request {
//   base_url: string
//   headers: map<string, string>
//   query_params: map<string, string>
// }
//
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
//
// enum std::HttpMethod {
//   Get
// }