function demo(s: string) -> int {
    baml.json.decode<int>(s) catch (e) {
    baml.json.ParseError => 0,
    baml.json.DecodeError => 0,
  };
    42
}
