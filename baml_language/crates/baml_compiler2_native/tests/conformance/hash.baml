// `baml.hash.Hasher.write` for the built-in hasher: the bytes of a
// `uint8array`, read in place into a state the collector moves.

function hash_write_nothing() -> bool {
  let written = baml.hash.DefaultHasher.new();
  written.write(b"");
  written.finish() == baml.hash.DefaultHasher.new().finish()
}
function hash_write() -> int {
  let hasher = baml.hash.DefaultHasher.new();
  hasher.write(u8_counted(100, 3));
  hasher.finish()
}
function hash_write_one_byte() -> int {
  let hasher = baml.hash.DefaultHasher.new();
  hasher.write(u8_counted(1, 1));
  hasher.finish()
}
// Writing a stream in pieces is writing it whole.
function hash_write_in_pieces() -> bool {
  let bytes = u8_counted(1000, 7);
  let whole = baml.hash.DefaultHasher.new();
  whole.write(bytes);
  let pieces = baml.hash.DefaultHasher.new();
  for (let i = 0; i < 1000; i += 13) {
    pieces.write(bytes.slice(i, i + 13));
  };
  whole.finish() == pieces.finish()
}
function hash_write_differs() -> bool {
  let one = baml.hash.DefaultHasher.new();
  one.write(u8_counted(9, 1));
  let other = baml.hash.DefaultHasher.new();
  other.write(u8_counted(9, 2));
  one.finish() == other.finish()
}
function hash_write_after_finish() -> string {
  let hasher = baml.hash.DefaultHasher.new();
  hasher.write(u8_counted(5, 1));
  let first = hasher.finish();
  hasher.write(u8_counted(5, 1));
  `${first} ${hasher.finish()}`
}
function hash_write_utf8() -> int {
  let hasher = baml.hash.DefaultHasher.new();
  hasher.write(rep("héllo", 3).to_utf8());
  hasher.finish()
}
