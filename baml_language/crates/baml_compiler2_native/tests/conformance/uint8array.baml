// `uint8array`: the literal, the length, reading and writing a byte, `==`,
// every method of `baml.Uint8Array`, and `string.to_utf8` and
// `string.from_utf8`. A result is shown as hex, since an entry returns no
// `uint8array`.

// The bytes 0, step, 2 * step, ... stored one at a time, so that the array
// is made and filled on the heap at run time.
function u8_counted(n: int, step: int) -> uint8array {
  let out = uint8array.zeroes(n);
  for (let i = 0; i < n; i += 1) { out[i] = i * step; };
  out
}

function u8_literal() -> string {
  b"hello".to_hex()
}
function u8_literal_escapes() -> string {
  b"\x00\xff\n".to_hex()
}
function u8_literal_empty() -> int {
  b"".length()
}
// A literal is a new array each time it is evaluated.
function u8_literal_is_fresh() -> string {
  let seen = "";
  for (let i = 0; i < 3; i += 1) {
    let bytes = b"ab";
    bytes[i % 2] = 122;
    seen = `${seen}${bytes.to_hex()} `;
  };
  seen
}

function u8_length() -> int {
  u8_counted(1000, 1).length()
}
function u8_index() -> int {
  let bytes = u8_counted(10, 3);
  bytes[0] + bytes[9] * 100 + bytes[-1] * 10000 + bytes[-10] * 1000000
}
function u8_index_past_end() -> int {
  u8_counted(3, 1)[3]
}
function u8_index_before_start() -> int {
  u8_counted(3, 1)[-4]
}
function u8_index_empty() -> int {
  u8_counted(0, 1)[0]
}
function u8_store() -> string {
  let bytes = u8_counted(4, 1);
  bytes[0] = 255;
  bytes[-1] = 16;
  bytes.to_hex()
}
// A stored value keeps its low eight bits.
function u8_store_masks() -> string {
  let bytes = u8_counted(3, 1);
  bytes[0] = 256;
  bytes[1] = -1;
  bytes[2] = 511;
  bytes.to_hex()
}
function u8_store_past_end() -> int {
  let bytes = u8_counted(3, 1);
  bytes[3] = 1;
  bytes.length()
}
function u8_loop_sum() -> int {
  let bytes = u8_counted(300, 7);
  let sum = 0;
  for (let i = 0; i < bytes.length(); i += 1) { sum += bytes[i]; };
  sum
}

function u8_equal() -> bool {
  u8_counted(20, 3) == u8_counted(20, 3)
}
function u8_equal_itself() -> bool {
  let bytes = u8_counted(5, 1);
  bytes == bytes
}
function u8_equal_empty() -> bool {
  u8_counted(0, 1) == b""
}
function u8_not_equal_length() -> bool {
  u8_counted(20, 3) == u8_counted(21, 3)
}
function u8_not_equal_byte() -> bool {
  let bytes = u8_counted(20, 3);
  bytes[19] = 0;
  bytes != u8_counted(20, 3)
}

function u8_at() -> string {
  let bytes = u8_counted(5, 10);
  `${bytes.at(0) ?? -1} ${bytes.at(4) ?? -1} ${bytes.at(-1) ?? -1} ${bytes.at(-5) ?? -1}`
}
function u8_at_outside() -> string {
  let bytes = u8_counted(5, 10);
  `${bytes.at(5) == null} ${bytes.at(-6) == null} ${b"".at(0) == null}`
}

function u8_store_many() -> string {
  let bytes = u8_counted(5000, 1);
  `${bytes.length()} ${bytes[4999]} ${bytes.slice(254, 258).to_hex()}`
}

function u8_concat() -> string {
  u8_counted(3, 1).concat(u8_counted(3, 16)).to_hex()
}
function u8_concat_empty() -> string {
  `${b"".concat(u8_counted(2, 1)).to_hex()} ${u8_counted(2, 1).concat(b"").to_hex()} ${b"".concat(b"").length()}`
}
function u8_concat_itself() -> string {
  let bytes = u8_counted(3, 1);
  let twice = bytes.concat(bytes);
  bytes[0] = 9;
  `${twice.to_hex()} ${bytes.to_hex()}`
}
function u8_concat_large() -> int {
  u8_counted(3000, 1).concat(u8_counted(3000, 1)).length()
}

function u8_includes() -> string {
  let bytes = u8_counted(10, 2);
  `${bytes.includes(0)} ${bytes.includes(18)} ${bytes.includes(3)} ${bytes.includes(256)} ${bytes.includes(-1)}`
}
function u8_index_of() -> string {
  let bytes = u8_counted(300, 1);
  `${bytes.index_of(0) ?? -1} ${bytes.index_of(255) ?? -1} ${bytes.index_of(256) == null} ${bytes.index_of(-1) == null} ${b"".index_of(0) == null}`
}

function u8_reverse() -> string {
  let bytes = u8_counted(5, 1);
  let reversed = bytes.reverse();
  `${reversed.to_hex()} ${bytes.to_hex()} ${b"".reverse().length()}`
}

function u8_slice() -> string {
  let bytes = u8_counted(8, 1);
  `${bytes.slice(0, 3).to_hex()} ${bytes.slice(-3, -1).to_hex()} ${bytes.slice(2, 100).to_hex()} ${bytes.slice(-100, 2).to_hex()}`
}
function u8_slice_empty() -> string {
  let bytes = u8_counted(8, 1);
  `${bytes.slice(5, 2).length()} ${bytes.slice(8, 8).length()} ${bytes.slice(100, 200).length()} ${b"".slice(0, 1).length()}`
}
function u8_slice_is_a_copy() -> string {
  let bytes = u8_counted(4, 1);
  let part = bytes.slice(1, 3);
  bytes[1] = 255;
  part[1] = 7;
  `${part.to_hex()} ${bytes.to_hex()}`
}

function u8_zeroes() -> string {
  `${uint8array.zeroes(3).to_hex()} ${uint8array.zeroes(0).length()} ${uint8array.zeroes(100000).length()}`
}
function u8_zeroes_negative() -> int {
  uint8array.zeroes(0 - u8_counted(1, 1).length()).length()
}
function u8_zeroes_too_large() -> int {
  uint8array.zeroes(4611686018427387903).length()
}

function u8_from_array() -> string {
  let values: int[] = [];
  for (let i = 0; i < 6; i += 1) { values.push(i * 51); };
  uint8array.from_array(values).to_hex()
}
function u8_from_array_empty() -> int {
  let values: int[] = [];
  uint8array.from_array(values).length()
}
function u8_from_array_too_large() -> int {
  uint8array.from_array([1, 2, 256]).length()
}
function u8_from_array_negative() -> int {
  uint8array.from_array([-1]).length()
}
function u8_to_array() -> string {
  let values = u8_counted(4, 85).to_array();
  `${values.length()} ${values[0]} ${values[1]} ${values[2]} ${values[3]}`
}
function u8_to_array_empty() -> int {
  b"".to_array().length()
}

function u8_from_hex() -> string {
  let bytes = uint8array.from_hex(rep("deadBEEF", 2));
  `${bytes.length()} ${bytes[0]} ${bytes[7]} ${bytes.to_hex()}`
}
function u8_from_hex_empty() -> int {
  uint8array.from_hex(rep("ab", 0)).length()
}
function u8_from_hex_odd() -> int {
  uint8array.from_hex(rep("abc", 1)).length()
}
function u8_from_hex_bad_high() -> int {
  uint8array.from_hex(rep("00", 2) + "g0").length()
}
function u8_from_hex_bad_low() -> int {
  uint8array.from_hex(rep("00", 2) + "0g").length()
}
function u8_to_hex() -> string {
  u8_counted(18, 15).to_hex()
}
function u8_to_hex_empty() -> string {
  `[${b"".to_hex()}]`
}

function u8_from_base64() -> string {
  uint8array.from_base64(rep("aGVsbG8=", 1)).to_hex()
}
function u8_from_base64_empty() -> int {
  uint8array.from_base64(rep("a", 0)).length()
}
function u8_from_base64_no_padding() -> int {
  uint8array.from_base64(rep("aGVsbG8", 1)).length()
}
function u8_from_base64_url_safe() -> int {
  uint8array.from_base64(rep("-_-_", 1)).length()
}
function u8_from_base64_invalid() -> int {
  uint8array.from_base64(rep("!", 4)).length()
}
function u8_to_base64() -> string {
  `${u8_counted(1, 1).to_base64()} ${u8_counted(2, 255).to_base64()} ${u8_counted(3, 127).to_base64()} ${u8_counted(40, 13).to_base64()} [${b"".to_base64()}]`
}
function u8_base64_round_trip() -> bool {
  let bytes = u8_counted(256, 1);
  uint8array.from_base64(bytes.to_base64()) == bytes
}

function u8_to_string() -> string {
  b"hello, caf\xc3\xa9".to_string()
}
function u8_to_string_invalid() -> string {
  b"a\x80b\xff".to_string()
}
function u8_to_string_empty() -> string {
  `[${b"".to_string()}]`
}
function u8_interpolated() -> string {
  let bytes = u8_counted(2, 1);
  bytes[0] = 104;
  bytes[1] = 105;
  `<${bytes}>`
}

function u8_sort() -> string {
  let bytes = u8_counted(40, 37);
  bytes.sort();
  bytes.to_hex()
}
function u8_sort_empty() -> int {
  let bytes = b"";
  bytes.sort();
  bytes.length()
}

function u8_to_utf8() -> string {
  rep("é", 2).to_utf8().to_hex() + " " + rep("a", 0).to_utf8().to_hex()
}
function u8_from_utf8() -> string {
  string.from_utf8(rep("né", 3).to_utf8())
}
function u8_from_utf8_empty() -> string {
  `[${string.from_utf8(b"")}]`
}
function u8_from_utf8_invalid() -> string {
  string.from_utf8(rep("ab", 1).to_utf8().concat(b"\xff"))
}
function u8_from_utf8_truncated() -> string {
  string.from_utf8(rep("é", 1).to_utf8().slice(0, -1))
}

// Held in a class, an array and a union, and across collections.
class U8Holder {
  name string
  bytes uint8array
}
function u8_in_a_class() -> string {
  let holder = U8Holder { name: rep("n", 3), bytes: u8_counted(4, 1) };
  holder.bytes[3] = 255;
  let filler = rep("x", 200);
  `${holder.name} ${holder.bytes.to_hex()} ${filler.length()}`
}
function u8_in_an_array() -> string {
  let all: uint8array[] = [];
  for (let i = 0; i < 20; i += 1) { all.push(u8_counted(i, i)); };
  let total = 0;
  for (let bytes in all) { total += bytes.length(); };
  `${total} ${all[19].to_hex()}`
}
function u8_in_a_union() -> string {
  let values: (uint8array | string | int)[] = [u8_counted(2, 9), rep("s", 2), 7];
  let seen = "";
  for (let value in values) {
    seen = match (value) {
      let bytes: uint8array => `${seen}bytes:${bytes.to_hex()} `,
      let text: string => `${seen}string:${text} `,
      let number: int => `${seen}int:${number} `,
    };
  };
  seen
}
function u8_optional() -> string {
  let none: uint8array? = null;
  let some: uint8array? = u8_counted(2, 3);
  `${none == null} ${some == null} ${some?.to_hex() ?? "-"}`
}
function u8_union_interpolated() -> string {
  let values: (uint8array | int)[] = [u8_counted(2, 52), 7];
  `${values[0]} ${values[1]}`
}
function u8_thrown() -> int {
  throw u8_counted(3, 1)
}
function u8_thrown_in_a_class() -> int {
  throw U8Holder { name: rep("n", 2), bytes: u8_counted(4, 1) }
}
