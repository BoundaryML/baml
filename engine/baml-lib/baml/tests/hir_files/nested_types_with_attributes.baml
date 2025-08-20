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
//   tags: arraystring
//   status: (string | int) @stream.done
//   matrix: arrayarrayint
//   config: map<string, bool> @stream.needed
// }
//
// enum std::HttpMethod {
//   Get
// }
