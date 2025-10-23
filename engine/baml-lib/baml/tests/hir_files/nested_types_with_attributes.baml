class DataModel {
  tags string[]
  status (string | int) @stream.done
  matrix int[][]
  config map<string, bool> @stream.not_null
}

// class DataModel {
//   tags: array<string>
//   status: (string @stream.done | int @stream.done) @stream.done
//   matrix: array<array<int>>
//   config: map<string, bool> @stream.needed
// }
