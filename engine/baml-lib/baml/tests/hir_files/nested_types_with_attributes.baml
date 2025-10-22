class DataModel {
  tags string[]
  status (string | int) @stream.done
  matrix int[][]
  config map<string, bool> @stream.not_null
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
// class DataModel {
//   tags: array<string>
//   status: (string @stream.done | int @stream.done) @stream.done
//   matrix: array<array<int>>
//   config: map<string, bool> @stream.needed
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
//   -->  test.baml:8
//    | 
//  7 | 
//  8 | <<<<<<< HEAD
//  9 | // class baml.HttpRequest {
//    | 
