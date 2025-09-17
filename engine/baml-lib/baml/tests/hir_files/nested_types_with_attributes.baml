class DataModel {
  tags string[]
  status (string | int) @stream.done
  matrix int[][]
  config map<string, bool> @stream.not_null
}

// class std::Request {
//   base_url: string
//   headers: map<string, string>
//   query_params: map<string, string>
// }
//
// class DataModel {
//   tags: array<string>
//   status: (string @stream.done | int @stream.done) @stream.done
//   matrix: array<array<int>>
//   config: map<string, bool> @stream.needed
// }
//
// enum std::HttpMethod {
//   Get
// }
