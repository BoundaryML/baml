class DataModel {
  tags string[]
  status (string | int) @stream.done
  matrix int[][]
  config map<string, bool> @stream.not_null
}

// class baml.HttpRequest {
//   url: string
//   method: baml.HttpMethod
//   headers: (map<string, string> | null)
//   query_params: (map<string, string> | null)
//   json: (ANY | null)
// }
//
// class DataModel {
//   tags: array<string>
//   status: (string @stream.done | int @stream.done) @stream.done
//   matrix: array<array<int>>
//   config: map<string, bool> @stream.needed
// }
//
// enum baml.HttpMethod {
//   Get
//   Post
//   Put
//   Delete
//   Patch
// }
