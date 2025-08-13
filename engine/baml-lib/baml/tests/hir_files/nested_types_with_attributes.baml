class DataModel {
  tags string[]
  status (string | int) @stream.done
  matrix int[][]
  config map<string, bool> @stream.not_null
} 