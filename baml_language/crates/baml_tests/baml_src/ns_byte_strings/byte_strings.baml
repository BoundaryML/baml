// Tests for byte string literals (`b"..."`) and uint8array behavior.
//
// Converted from crates/baml_tests/tests/byte_strings.rs.
//
// In test blocks, a bare local read (`let x = b"..."; x`) hits the VM boxing
// bug — the result compares unequal to any literal.  Those tests use top-level
// helper functions where locals are not boxed.
//
// SKIPPED:
//   - `pass_as_argument` (B5): requires external host-provided args.
//
// Re-added (were wrongly believed impossible):
//   - `index_out_of_bounds`: throws baml.panics.IndexOutOfBounds; caught via
//     a helper function to avoid the test-block local-boxing bug (direct
//     b"a"[5] in a test block does not throw due to boxing).
//   - `index_negative`: same class.

// ─── helpers for tests that read bare locals ──────────────────────────────────

// B1. Byte string literal construction — `let x = b"..."; x` hits the boxing
// bug in test blocks; move into functions.

function byte_strings_literal_basic() -> uint8array {
    let x = b"hello";
    x
}

function byte_strings_literal_empty() -> uint8array {
    let x = b"";
    x
}

function byte_strings_literal_hex_escapes() -> uint8array {
    let x = b"\x00\x7f\xff";
    x
}

function byte_strings_literal_standard_escapes() -> uint8array {
    let x = b"\n\t\r\0\\\"";
    x
}

function byte_strings_literal_mixed() -> uint8array {
    let x = b"A\x42C";
    x
}

// B3. zeroes
function byte_strings_zeroes_basic() -> uint8array {
    let x = b"\x00\x00\x00";
    x
}

// B4. Index read via []
function byte_strings_index_read() -> int {
    let data = b"\x41\x42";
    data[0]
}

function byte_strings_index_write() -> int? {
    let data = b"\x00\x00";
    data[0] = 255;
    data.at(0)
}

function byte_strings_index_write_out_of_range() -> int {
    let data = b"\x00";
    data[0] = 256;
    data[0]
}

// B5. return uint8array
function byte_strings_return_uint8array() -> uint8array {
    let x = b"abc";
    x
}

// B7. `b` used as an identifier (not a byte-string prefix)
function byte_strings_b_as_identifier() -> int {
    let b = 42;
    b
}

// B9. Direct index read
function byte_strings_index_read_direct() -> int {
    let data = b"\x41\x42\x43";
    data[0]
}

// B4 (error paths). Direct b"a"[N] in a test block is affected by the boxing
// bug and does not throw.  Wrapping in a function avoids the bug.
function byte_strings_index_oob() -> int {
    b"a"[5]
}

// b"a" = [97]; [-1] counts from the end → the last (only) byte.
function byte_strings_index_neg_in_bounds() -> int {
    b"a"[-1]
}

// [-2] is past the start of a length-1 array (it normalizes to -1).
function byte_strings_index_before_start() -> int {
    b"a"[-2]
}

// B8. Helper function defined inline alongside main — used by literal_as_call_argument.
function get_len(data: uint8array) -> int {
    data.length()
}

// ─── B1. Byte string literal construction ─────────────────────────────────────

test "literal_basic" {
    assert.equal(byte_strings_literal_basic(), b"hello")
}

test "literal_empty" {
    assert.equal(byte_strings_literal_empty(), b"")
}

test "literal_hex_escapes" {
    assert.equal(byte_strings_literal_hex_escapes(), b"\x00\x7f\xff")
}

test "literal_standard_escapes" {
    assert.equal(byte_strings_literal_standard_escapes(), b"\n\t\r\0\\\"")
}

test "literal_mixed_ascii_and_hex" {
    assert.equal(byte_strings_literal_mixed(), b"A\x42C")
}

// ─── B2. Uint8Array methods ───────────────────────────────────────────────────

test "length_basic" {
    assert.equal(b"hello".length(), 5)
}

test "length_empty" {
    assert.equal(b"".length(), 0)
}

// at() returns int?; 0x42 = 66 decimal.
test "at_valid_index" {
    assert.equal(b"\x41\x42\x43".at(1), 66)
}

test "at_out_of_bounds" {
    assert.equal(b"abc".at(10), null)
}

// b"abc" = [97, 98, 99]; .at(-1) counts from the end → the last byte.
test "at_negative_index" {
    assert.equal(b"abc".at(-1), 99)
}

// Past the start (b"abc".at(-4) normalizes to -1) → null, like Array.at.
test "at_negative_out_of_bounds" {
    assert.equal(b"abc".at(-4), null)
}

// ─── B3. Uint8Array.zeroes (literal only) ─────────────────────────────────────

test "zeroes_basic" {
    assert.equal(byte_strings_zeroes_basic(), b"\x00\x00\x00")
}

// ─── B4. Index read/write via [] ──────────────────────────────────────────────

// 0x41 = 65 decimal ('A')
test "index_read" {
    assert.equal(byte_strings_index_read(), 65)
}

test "index_write" {
    assert.equal(byte_strings_index_write(), 255)
}

// Writing 256 wraps/clamps to 0.
test "index_write_out_of_range" {
    assert.equal(byte_strings_index_write_out_of_range(), 0)
}

test "index_out_of_bounds" {
    {
        byte_strings_index_oob();
        baml.sys.panic("expected throw")
    } catch (e) {
        baml.panics.IndexOutOfBounds => null
    }
}

// [-1] counts from the end and lands in bounds → the last byte (no panic).
test "index_negative_in_bounds" {
    assert.equal(byte_strings_index_neg_in_bounds(), 97)
}

// An index past the start raises IndexOutOfBounds, unlike Array.at.
test "index_before_start" {
    {
        byte_strings_index_before_start();
        baml.sys.panic("expected throw")
    } catch (e) {
        baml.panics.IndexOutOfBounds => null
    }
}

// ─── B5. Type identity & passing as arguments ─────────────────────────────────

// pass_as_argument: SKIPPED — requires host-provided external args.

test "return_uint8array" {
    assert.equal(byte_strings_return_uint8array(), b"abc")
}

// ─── B6. Deep equality ────────────────────────────────────────────────────────

test "equality_same_content" {
    assert.is_true(b"abc" == b"abc")
}

test "equality_different_content" {
    assert.is_true(b"abc" != b"xyz")
}

test "equality_empty" {
    assert.is_true(b"" == b"")
}

test "equality_different_lengths" {
    assert.is_true(b"ab" != b"abc")
}

test "equality_different_lengths_reversed" {
    assert.is_true(b"abc" != b"ab")
}

test "inequality_same" {
    assert.is_true(!(b"abc" != b"abc"))
}

test "inequality_different" {
    assert.is_true(b"abc" != b"xyz")
}

// ─── B7. Disambiguation: `b` as identifier vs byte-string prefix ───────────────

test "b_as_identifier" {
    assert.equal(byte_strings_b_as_identifier(), 42)
}

// ─── B8. Byte string as call argument ─────────────────────────────────────────

test "literal_as_call_argument" {
    assert.equal(get_len(b"hello"), 5)
}

// ─── B9. Direct index read (data[0] not .at(0)) ───────────────────────────────

// 0x41 = 65 decimal ('A')
test "index_read_direct" {
    assert.equal(byte_strings_index_read_direct(), 65)
}
