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

function byte_strings_index_neg() -> int {
    b"a"[-1]
}

// B8. Helper function defined inline alongside main — used by literal_as_call_argument.
function get_len(data: uint8array) -> int {
    data.length()
}

// ─── B1. Byte string literal construction ─────────────────────────────────────

test "literal_basic" {
    assert.is_true(baml.deep_equals(byte_strings_literal_basic(), b"hello"))
}

test "literal_empty" {
    assert.is_true(baml.deep_equals(byte_strings_literal_empty(), b""))
}

test "literal_hex_escapes" {
    assert.is_true(baml.deep_equals(byte_strings_literal_hex_escapes(), b"\x00\x7f\xff"))
}

test "literal_standard_escapes" {
    assert.is_true(baml.deep_equals(byte_strings_literal_standard_escapes(), b"\n\t\r\0\\\""))
}

test "literal_mixed_ascii_and_hex" {
    assert.is_true(baml.deep_equals(byte_strings_literal_mixed(), b"A\x42C"))
}

// ─── B2. Uint8Array methods ───────────────────────────────────────────────────

test "length_basic" {
    assert.is_true(baml.deep_equals(b"hello".length(), 5))
}

test "length_empty" {
    assert.is_true(baml.deep_equals(b"".length(), 0))
}

// at() returns int?; 0x42 = 66 decimal.
test "at_valid_index" {
    assert.equal(b"\x41\x42\x43".at(1), 66)
}

test "at_out_of_bounds" {
    assert.equal(b"abc".at(10), null)
}

test "at_negative_index" {
    assert.equal(b"abc".at(-1), null)
}

// ─── B3. Uint8Array.zeroes (literal only) ─────────────────────────────────────

test "zeroes_basic" {
    assert.is_true(baml.deep_equals(byte_strings_zeroes_basic(), b"\x00\x00\x00"))
}

// ─── B4. Index read/write via [] ──────────────────────────────────────────────

// 0x41 = 65 decimal ('A')
test "index_read" {
    assert.is_true(baml.deep_equals(byte_strings_index_read(), 65))
}

test "index_write" {
    assert.equal(byte_strings_index_write(), 255)
}

// Writing 256 wraps/clamps to 0.
test "index_write_out_of_range" {
    assert.is_true(baml.deep_equals(byte_strings_index_write_out_of_range(), 0))
}

test "index_out_of_bounds" {
    {
        byte_strings_index_oob();
        baml.sys.panic("expected throw")
    } catch (e) {
        baml.panics.IndexOutOfBounds => null
    }
}

test "index_negative" {
    {
        byte_strings_index_neg();
        baml.sys.panic("expected throw")
    } catch (e) {
        baml.panics.IndexOutOfBounds => null
    }
}

// ─── B5. Type identity & passing as arguments ─────────────────────────────────

// pass_as_argument: SKIPPED — requires host-provided external args.

test "return_uint8array" {
    assert.is_true(baml.deep_equals(byte_strings_return_uint8array(), b"abc"))
}

// ─── B6. Deep equality ────────────────────────────────────────────────────────

test "equality_same_content" {
    assert.is_true(baml.deep_equals(b"abc" == b"abc", true))
}

test "equality_different_content" {
    assert.is_true(baml.deep_equals(b"abc" == b"xyz", false))
}

test "equality_empty" {
    assert.is_true(baml.deep_equals(b"" == b"", true))
}

test "equality_different_lengths" {
    assert.is_true(baml.deep_equals(b"ab" == b"abc", false))
}

test "equality_different_lengths_reversed" {
    assert.is_true(baml.deep_equals(b"abc" == b"ab", false))
}

test "inequality_same" {
    assert.is_true(baml.deep_equals(b"abc" != b"abc", false))
}

test "inequality_different" {
    assert.is_true(baml.deep_equals(b"abc" != b"xyz", true))
}

// ─── B7. Disambiguation: `b` as identifier vs byte-string prefix ───────────────

test "b_as_identifier" {
    assert.is_true(baml.deep_equals(byte_strings_b_as_identifier(), 42))
}

// ─── B8. Byte string as call argument ─────────────────────────────────────────

test "literal_as_call_argument" {
    assert.is_true(baml.deep_equals(get_len(b"hello"), 5))
}

// ─── B9. Direct index read (data[0] not .at(0)) ───────────────────────────────

// 0x41 = 65 decimal ('A')
test "index_read_direct" {
    assert.is_true(baml.deep_equals(byte_strings_index_read_direct(), 65))
}
