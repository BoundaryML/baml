// Tests for string operations.
//
// Converted from crates/baml_tests/tests/strings.rs.
//
// In test blocks, `a + b` where both a and b are locals hits the VM boxing bug
// (`CannotApplyBinOp { left: Object(Any), right: Object(Any) }`). Those cases
// use top-level helper functions. Method-call results (including calls on a
// local receiver, e.g. `s.length()`) are fine inline.

// ─── helpers ─────────────────────────────────────────────────────────────────

function strings_concat() -> string {
    let a = "Hello";
    let b = " World";
    a + b
}

// Throw-test helpers: the catch base expression must start with a `let` or a
// non-string-literal expression; otherwise the parser treats the opening `{` as
// a map literal when the first token is a string literal.

function strings_char_at(index: int) -> string {
    "hello".char_at(index)
}

function strings_from_utf8_invalid() -> string {
    string.from_utf8(b"\xFF")
}

function strings_from_code_points_negative() -> string {
    string.from_code_points([-1])
}

function strings_from_code_points_surrogate() -> string {
    string.from_code_points([55296])
}

function strings_from_code_points_too_large() -> string {
    string.from_code_points([1114112])
}

// Helpers that build strings containing control characters not representable
// as BAML string-literal escape sequences (only \n \t \r \0 \\ \" are
// recognized by the BAML lexer).
function strings_form_feed() -> string {
    // \x0C = form feed
    string.from_utf8(b"\x0C")
}

function strings_low_controls() -> string {
    // \x00 \x01 \x1F
    string.from_utf8(b"\x00\x01\x1F")
}

function strings_del() -> string {
    // \x7F = DEL
    string.from_utf8(b"\x7F")
}

// U+00A0 NO-BREAK SPACE, encoded as UTF-8: 0xC2 0xA0.
// BAML string literals don't support \u{...} Unicode escapes.
function strings_nbsp() -> string {
    string.from_utf8(b"\xC2\xA0")
}

// ─── concat ──────────────────────────────────────────────────────────────────

test "concat_strings" {
    assert.is_true(baml.deep_equals(strings_concat(), "Hello World"))
}

// ─── comparisons ─────────────────────────────────────────────────────────────

test "string_equality_true" {
    assert.is_true(baml.deep_equals("Hello" == "Hello", true))
}

test "string_equality_false" {
    assert.is_true(baml.deep_equals("Hello" == "World", false))
}

test "string_not_equal_true" {
    assert.is_true(baml.deep_equals("Hello" != "World", true))
}

test "string_less_than" {
    assert.is_true(baml.deep_equals("a" < "b", true))
}

test "string_less_than_or_equal" {
    assert.is_true(baml.deep_equals("a" <= "b", true))
}

test "string_greater_than" {
    assert.is_true(baml.deep_equals("b" > "a", true))
}

test "string_greater_than_or_equal" {
    assert.is_true(baml.deep_equals("b" >= "a", true))
}

// ─── basic methods ────────────────────────────────────────────────────────────

test "string_length" {
    assert.is_true(baml.deep_equals("hello".length(), 5))
}

test "string_to_lower_case" {
    assert.is_true(baml.deep_equals("HELLO World".to_lower_case(), "hello world"))
}

test "string_to_upper_case" {
    assert.is_true(baml.deep_equals("hello WORLD".to_upper_case(), "HELLO WORLD"))
}

test "string_trim" {
    assert.is_true(baml.deep_equals("  hello world  ".trim(), "hello world"))
}

test "string_includes" {
    assert.is_true(baml.deep_equals("hello world".includes("world"), true))
}

test "string_starts_with" {
    assert.is_true(baml.deep_equals("hello world".starts_with("hello"), true))
}

test "string_ends_with" {
    assert.is_true(baml.deep_equals("hello world".ends_with("world"), true))
}

test "string_split" {
    assert.is_true(baml.deep_equals("hello,world,test".split(","), ["hello", "world", "test"]))
}

test "string_substring" {
    assert.is_true(baml.deep_equals("hello world".substring(0, 5), "hello"))
}

test "string_substring_bounds" {
    // end byte offset clamped to length
    assert.is_true(baml.deep_equals("hello".substring(2, 10), "llo"))
}

// ─── codepoint-indexed semantics ─────────────────────────────────────────────

// length() returns codepoints, not bytes.
test "string_length_codepoints_non_ascii" {
    assert.is_true(baml.deep_equals("héllo".length(), 5))
}

test "string_length_codepoints_emoji" {
    assert.is_true(baml.deep_equals("🐈".length(), 1))
}

// byte_length() returns UTF-8 bytes.
test "string_byte_length_non_ascii" {
    assert.is_true(baml.deep_equals("héllo".byte_length(), 6))
}

test "string_byte_length_emoji" {
    assert.is_true(baml.deep_equals("🐈".byte_length(), 4))
}

// substring uses codepoint indices.
test "string_substring_codepoint_indexed" {
    assert.is_true(baml.deep_equals("héllo".substring(0, 2), "hé"))
}

test "string_substring_emoji_first_char" {
    assert.is_true(baml.deep_equals("😀hello".substring(0, 1), "😀"))
}

test "string_substring_after_emoji" {
    assert.is_true(baml.deep_equals("😀hello".substring(1, 4), "hel"))
}

// Negative indices count from the end (length 5): -3 → 2, -1 → 4.
test "string_substring_negative_counts_from_end" {
    assert.is_true(baml.deep_equals("hello".substring(-3, 4), "ll"));
    assert.is_true(baml.deep_equals("hello".substring(-3, -1), "ll"))
}

// A negative index past the start clamps to 0.
test "string_substring_negative_clamps_to_zero" {
    assert.is_true(baml.deep_equals("hello".substring(-100, 4), "hell"))
}

test "string_substring_empty_when_start_past_end" {
    assert.is_true(baml.deep_equals("hello".substring(4, 2), ""))
}

test "string_substring_out_of_bounds_clamps" {
    assert.is_true(baml.deep_equals("hello".substring(0, 100), "hello"))
}

// char_at uses codepoint index.
test "string_char_at_multibyte" {
    assert.is_true(baml.deep_equals("héllo".char_at(1), "é"))
}

test "string_char_at_after_multibyte" {
    assert.is_true(baml.deep_equals("héllo".char_at(2), "l"))
}

test "string_char_at_emoji" {
    assert.is_true(baml.deep_equals("😀hello".char_at(0), "😀"))
}

test "string_char_at_after_emoji" {
    assert.is_true(baml.deep_equals("😀hello".char_at(1), "h"))
}

// char_at at or past the end raises IndexOutOfBounds ("hello" has length 5).
test "string_char_at_at_length_panics" {
    {
        strings_char_at(5);
        baml.sys.panic("expected char_at to throw")
    } catch (e) {
        baml.panics.IndexOutOfBounds => null
    }
}

test "string_char_at_past_end_panics" {
    {
        strings_char_at(99);
        baml.sys.panic("expected char_at to throw")
    } catch (e) {
        baml.panics.IndexOutOfBounds => null
    }
}

// Negative indices count from the end: -1 is the last character.
test "string_char_at_negative_counts_from_end" {
    assert.is_true(baml.deep_equals("hello".char_at(-1), "o"));
    // "héllo" = h é l l o; -4 normalizes to codepoint 1.
    assert.is_true(baml.deep_equals("héllo".char_at(-4), "é"))
}

// Past the start (normalizes before index 0) also raises IndexOutOfBounds.
test "string_char_at_past_start_panics" {
    {
        strings_char_at(-6);
        baml.sys.panic("expected char_at to throw")
    } catch (e) {
        baml.panics.IndexOutOfBounds => null
    }
}

// index_of returns codepoint index.
test "string_index_of_returns_codepoint_index" {
    assert.is_true(baml.deep_equals("héllo".index_of("l"), 2))
}

test "string_index_of_after_emoji" {
    assert.is_true(baml.deep_equals("😀hello".index_of("h"), 1))
}

// ─── repeat ──────────────────────────────────────────────────────────────────

test "string_repeat" {
    assert.is_true(baml.deep_equals("ab".repeat(3), "ababab"))
}

test "string_repeat_zero" {
    assert.is_true(baml.deep_equals("ab".repeat(0), ""))
}

test "string_repeat_negative" {
    assert.is_true(baml.deep_equals("ab".repeat(-1), ""))
}

// ─── char_count ───────────────────────────────────────────────────────────────

test "string_char_count_ascii" {
    assert.is_true(baml.deep_equals("hello".char_count(), 5))
}

test "string_char_count_multibyte" {
    assert.is_true(baml.deep_equals("héllo".char_count(), 5))
}

test "string_char_count_emoji" {
    assert.is_true(baml.deep_equals("🐑🐑".char_count(), 2))
}

test "string_char_count_empty" {
    assert.is_true(baml.deep_equals("".char_count(), 0))
}

// ─── trim_start / trim_end ────────────────────────────────────────────────────

test "string_trim_start_removes_leading_only" {
    assert.is_true(baml.deep_equals("  hi  ".trim_start(), "hi  "))
}

test "string_trim_end_removes_trailing_only" {
    assert.is_true(baml.deep_equals("  hi  ".trim_end(), "  hi"))
}

test "string_trim_start_handles_newlines_and_tabs" {
    assert.is_true(baml.deep_equals("\n\t hi".trim_start(), "hi"))
}

test "string_trim_end_no_whitespace_unchanged" {
    assert.is_true(baml.deep_equals("hi".trim_end(), "hi"))
}

test "string_trim_start_empty" {
    assert.is_true(baml.deep_equals("".trim_start(), ""))
}

// ─── lines ────────────────────────────────────────────────────────────────────

test "string_lines_lf" {
    assert.is_true(baml.deep_equals("a\nb\nc".lines(), ["a", "b", "c"]))
}

test "string_lines_crlf" {
    assert.is_true(baml.deep_equals("a\r\nb\r\nc".lines(), ["a", "b", "c"]))
}

// Trailing newline does not add an empty entry.
test "string_lines_trailing_newline_no_empty" {
    assert.is_true(baml.deep_equals("a\nb\n".lines(), ["a", "b"]))
}

test "string_lines_empty_string" {
    // "".lines() is a call result; compare its length instead of against an
    // empty-array local (which hits the VM boxing bug).
    assert.is_true(baml.deep_equals("".lines().length(), 0))
}

// A single "\n" produces one empty string (the line before the newline).
test "string_lines_just_newline" {
    assert.is_true(baml.deep_equals("\n".lines(), [""]))
}

// ─── to_utf8 / from_utf8 ──────────────────────────────────────────────────────

// "hi" → [0x68, 0x69]
test "string_to_utf8_ascii" {
    assert.is_true(baml.deep_equals("hi".to_utf8(), b"\x68\x69"))
}

// "é" (U+00E9) → [0xC3, 0xA9] in UTF-8
test "string_to_utf8_multibyte" {
    assert.is_true(baml.deep_equals("é".to_utf8(), b"\xC3\xA9"))
}

test "string_to_utf8_empty" {
    assert.is_true(baml.deep_equals("".to_utf8(), b""))
}

test "string_from_utf8_ascii_round_trip" {
    assert.is_true(baml.deep_equals(string.from_utf8(b"\x68\x69"), "hi"))
}

test "string_from_utf8_multibyte_round_trip" {
    assert.is_true(baml.deep_equals(string.from_utf8(b"\xC3\xA9"), "é"))
}

// 0xFF is never a valid UTF-8 byte.
test "string_from_utf8_invalid_throws" {
    {
        let _ = strings_from_utf8_invalid();
        baml.sys.panic("expected from_utf8 to throw")
    } catch (e) {
        baml.errors.InvalidArgument => null
    }
}

test "string_from_utf8_empty" {
    assert.is_true(baml.deep_equals(string.from_utf8(b""), ""))
}

// ─── from_code_points ─────────────────────────────────────────────────────────

test "string_from_code_points_ascii" {
    assert.is_true(baml.deep_equals(string.from_code_points([104, 105]), "hi"))
}

test "string_from_code_points_multibyte" {
    // 233 = U+00E9 = é
    assert.is_true(baml.deep_equals(string.from_code_points([233]), "é"))
}

test "string_from_code_points_emoji" {
    // 128017 = U+1F411 = 🐑
    assert.is_true(baml.deep_equals(string.from_code_points([128017]), "🐑"))
}

test "string_from_code_points_empty" {
    assert.is_true(baml.deep_equals(string.from_code_points([]), ""))
}

test "string_from_code_points_negative_throws" {
    {
        let _ = strings_from_code_points_negative();
        baml.sys.panic("expected from_code_points to throw")
    } catch (e) {
        baml.errors.InvalidArgument => null
    }
}

// U+D800 (55296) is the start of the high-surrogate range; not a valid scalar.
test "string_from_code_points_surrogate_throws" {
    {
        let _ = strings_from_code_points_surrogate();
        baml.sys.panic("expected from_code_points to throw")
    } catch (e) {
        baml.errors.InvalidArgument => null
    }
}

// U+110000 (1114112) is one past the maximum Unicode code point.
test "string_from_code_points_too_large_throws" {
    {
        let _ = strings_from_code_points_too_large();
        baml.sys.panic("expected from_code_points to throw")
    } catch (e) {
        baml.errors.InvalidArgument => null
    }
}

// ─── Character-class predicates (BEP-043) ─────────────────────────────────────

// ── is_numeric (Unicode) ──

test "string_is_numeric_ascii_digits" {
    assert.is_true(baml.deep_equals("12345".is_numeric(), true))
}

// U+2167 ROMAN NUMERAL EIGHT — Unicode general category Nl.
test "string_is_numeric_unicode_roman" {
    assert.is_true(baml.deep_equals("Ⅷ".is_numeric(), true))
}

test "string_is_numeric_mixed_false" {
    assert.is_true(baml.deep_equals("12a".is_numeric(), false))
}

test "string_is_numeric_letters_false" {
    assert.is_true(baml.deep_equals("abc".is_numeric(), false))
}

test "string_is_numeric_empty_true" {
    assert.is_true(baml.deep_equals("".is_numeric(), true))
}

// ── is_alphabetic (Unicode) ──

test "string_is_alphabetic_ascii" {
    assert.is_true(baml.deep_equals("hello".is_alphabetic(), true))
}

test "string_is_alphabetic_accented" {
    assert.is_true(baml.deep_equals("héllo".is_alphabetic(), true))
}

// 漢字 — common CJK letters.
test "string_is_alphabetic_cjk" {
    assert.is_true(baml.deep_equals("漢字".is_alphabetic(), true))
}

test "string_is_alphabetic_with_digit_false" {
    assert.is_true(baml.deep_equals("abc1".is_alphabetic(), false))
}

test "string_is_alphabetic_with_space_false" {
    assert.is_true(baml.deep_equals("a b".is_alphabetic(), false))
}

test "string_is_alphabetic_empty_true" {
    assert.is_true(baml.deep_equals("".is_alphabetic(), true))
}

// ── is_alphanumeric (Unicode) ──

test "string_is_alphanumeric_letters_and_digits" {
    assert.is_true(baml.deep_equals("abc123".is_alphanumeric(), true))
}

test "string_is_alphanumeric_accented_with_digit" {
    assert.is_true(baml.deep_equals("héllo7".is_alphanumeric(), true))
}

test "string_is_alphanumeric_with_space_false" {
    assert.is_true(baml.deep_equals("a b".is_alphanumeric(), false))
}

test "string_is_alphanumeric_with_punct_false" {
    assert.is_true(baml.deep_equals("a!b".is_alphanumeric(), false))
}

test "string_is_alphanumeric_empty_true" {
    assert.is_true(baml.deep_equals("".is_alphanumeric(), true))
}

// ── is_uppercase / is_lowercase (Unicode) ──

test "string_is_uppercase_all_upper" {
    assert.is_true(baml.deep_equals("HELLO".is_uppercase(), true))
}

test "string_is_uppercase_mixed_false" {
    assert.is_true(baml.deep_equals("Hello".is_uppercase(), false))
}

// Digits are not uppercase.
test "string_is_uppercase_digit_false" {
    assert.is_true(baml.deep_equals("1".is_uppercase(), false))
}

test "string_is_uppercase_empty_true" {
    assert.is_true(baml.deep_equals("".is_uppercase(), true))
}

test "string_is_lowercase_all_lower" {
    assert.is_true(baml.deep_equals("hello".is_lowercase(), true))
}

test "string_is_lowercase_mixed_false" {
    assert.is_true(baml.deep_equals("Hello".is_lowercase(), false))
}

test "string_is_lowercase_accented_lower" {
    assert.is_true(baml.deep_equals("héllo".is_lowercase(), true))
}

test "string_is_lowercase_empty_true" {
    assert.is_true(baml.deep_equals("".is_lowercase(), true))
}

// ── is_whitespace (Unicode) ──

test "string_is_whitespace_spaces" {
    assert.is_true(baml.deep_equals("   ".is_whitespace(), true))
}

test "string_is_whitespace_mixed_ws" {
    assert.is_true(baml.deep_equals(" \t\n\r".is_whitespace(), true))
}

// U+00A0 NO-BREAK SPACE — Unicode whitespace. BAML strings don't support
// \u{...} escapes, so we build the character from its UTF-8 bytes.
test "string_is_whitespace_unicode_nbsp" {
    assert.is_true(baml.deep_equals(strings_nbsp().is_whitespace(), true))
}

test "string_is_whitespace_with_letter_false" {
    assert.is_true(baml.deep_equals("a b".is_whitespace(), false))
}

test "string_is_whitespace_empty_true" {
    assert.is_true(baml.deep_equals("".is_whitespace(), true))
}

// ── is_control / is_graphic ──

test "string_is_control_newline_tab" {
    assert.is_true(baml.deep_equals("\n\t".is_control(), true))
}

test "string_is_control_letter_false" {
    assert.is_true(baml.deep_equals("a".is_control(), false))
}

test "string_is_control_empty_true" {
    assert.is_true(baml.deep_equals("".is_control(), true))
}

test "string_is_graphic_letters" {
    assert.is_true(baml.deep_equals("abc".is_graphic(), true))
}

test "string_is_graphic_accented" {
    assert.is_true(baml.deep_equals("héllo".is_graphic(), true))
}

test "string_is_graphic_with_space_false" {
    assert.is_true(baml.deep_equals("a b".is_graphic(), false))
}

test "string_is_graphic_with_newline_false" {
    assert.is_true(baml.deep_equals("a\n".is_graphic(), false))
}

test "string_is_graphic_empty_true" {
    assert.is_true(baml.deep_equals("".is_graphic(), true))
}

// ── is_ascii ──

test "string_is_ascii_pure_ascii" {
    assert.is_true(baml.deep_equals("hello world!".is_ascii(), true))
}

test "string_is_ascii_with_accent_false" {
    assert.is_true(baml.deep_equals("héllo".is_ascii(), false))
}

test "string_is_ascii_with_emoji_false" {
    assert.is_true(baml.deep_equals("🐑".is_ascii(), false))
}

// \x00 (\0) and \x7F (DEL) are both valid ASCII bytes.
// \x7F is not a BAML string escape, so we build the test string via bytes.
function strings_nul_and_del() -> string {
    string.from_utf8(b"\x00\x7F")
}

test "string_is_ascii_control_chars" {
    assert.is_true(baml.deep_equals(strings_nul_and_del().is_ascii(), true))
}

test "string_is_ascii_empty_true" {
    assert.is_true(baml.deep_equals("".is_ascii(), true))
}

// ── is_ascii_numeric ──

test "string_is_ascii_numeric_digits" {
    assert.is_true(baml.deep_equals("01234".is_ascii_numeric(), true))
}

// Unicode numeral Ⅷ is rejected by the ASCII variant.
test "string_is_ascii_numeric_unicode_numeral_false" {
    assert.is_true(baml.deep_equals("Ⅷ".is_ascii_numeric(), false))
}

test "string_is_ascii_numeric_letter_false" {
    assert.is_true(baml.deep_equals("1a".is_ascii_numeric(), false))
}

test "string_is_ascii_numeric_empty_true" {
    assert.is_true(baml.deep_equals("".is_ascii_numeric(), true))
}

// ── is_ascii_alphabetic / is_ascii_alphanumeric ──

test "string_is_ascii_alphabetic_letters" {
    assert.is_true(baml.deep_equals("Hello".is_ascii_alphabetic(), true))
}

test "string_is_ascii_alphabetic_accented_false" {
    assert.is_true(baml.deep_equals("héllo".is_ascii_alphabetic(), false))
}

test "string_is_ascii_alphabetic_with_digit_false" {
    assert.is_true(baml.deep_equals("abc1".is_ascii_alphabetic(), false))
}

test "string_is_ascii_alphabetic_empty_true" {
    assert.is_true(baml.deep_equals("".is_ascii_alphabetic(), true))
}

test "string_is_ascii_alphanumeric_basic" {
    assert.is_true(baml.deep_equals("abc123".is_ascii_alphanumeric(), true))
}

test "string_is_ascii_alphanumeric_accented_false" {
    assert.is_true(baml.deep_equals("héllo1".is_ascii_alphanumeric(), false))
}

test "string_is_ascii_alphanumeric_with_space_false" {
    assert.is_true(baml.deep_equals("a 1".is_ascii_alphanumeric(), false))
}

test "string_is_ascii_alphanumeric_empty_true" {
    assert.is_true(baml.deep_equals("".is_ascii_alphanumeric(), true))
}

// ── is_ascii_uppercase / is_ascii_lowercase ──

test "string_is_ascii_uppercase_pure_upper" {
    assert.is_true(baml.deep_equals("ABC".is_ascii_uppercase(), true))
}

test "string_is_ascii_uppercase_mixed_false" {
    assert.is_true(baml.deep_equals("Abc".is_ascii_uppercase(), false))
}

// É (U+00C9) is upper but not ASCII.
test "string_is_ascii_uppercase_with_accent_false" {
    assert.is_true(baml.deep_equals("É".is_ascii_uppercase(), false))
}

test "string_is_ascii_uppercase_empty_true" {
    assert.is_true(baml.deep_equals("".is_ascii_uppercase(), true))
}

test "string_is_ascii_lowercase_pure_lower" {
    assert.is_true(baml.deep_equals("abc".is_ascii_lowercase(), true))
}

test "string_is_ascii_lowercase_mixed_false" {
    assert.is_true(baml.deep_equals("aBc".is_ascii_lowercase(), false))
}

// é is lower but not ASCII.
test "string_is_ascii_lowercase_with_accent_false" {
    assert.is_true(baml.deep_equals("héllo".is_ascii_lowercase(), false))
}

test "string_is_ascii_lowercase_empty_true" {
    assert.is_true(baml.deep_equals("".is_ascii_lowercase(), true))
}

// ── is_ascii_whitespace ──

test "string_is_ascii_whitespace_basic" {
    assert.is_true(baml.deep_equals(" \t\n\r".is_ascii_whitespace(), true))
}

// Form feed \x0C is ASCII whitespace. BAML strings don't support \x escapes,
// so we decode from bytes.
test "string_is_ascii_whitespace_form_feed" {
    assert.is_true(baml.deep_equals(strings_form_feed().is_ascii_whitespace(), true))
}

// NBSP (U+00A0) is whitespace per Unicode but not per ASCII.
test "string_is_ascii_whitespace_nbsp_false" {
    assert.is_true(baml.deep_equals(strings_nbsp().is_ascii_whitespace(), false))
}

test "string_is_ascii_whitespace_letter_false" {
    assert.is_true(baml.deep_equals("a".is_ascii_whitespace(), false))
}

test "string_is_ascii_whitespace_empty_true" {
    assert.is_true(baml.deep_equals("".is_ascii_whitespace(), true))
}

// ── is_ascii_control ──

// \x01 and \x1F are not representable as BAML string escapes; build via bytes.
test "string_is_ascii_control_low_range" {
    assert.is_true(baml.deep_equals(strings_low_controls().is_ascii_control(), true))
}

// DEL (0x7F) is ASCII control.
test "string_is_ascii_control_del" {
    assert.is_true(baml.deep_equals(strings_del().is_ascii_control(), true))
}

test "string_is_ascii_control_letter_false" {
    assert.is_true(baml.deep_equals("a".is_ascii_control(), false))
}

test "string_is_ascii_control_empty_true" {
    assert.is_true(baml.deep_equals("".is_ascii_control(), true))
}

// ── is_ascii_graphic ──

test "string_is_ascii_graphic_letters" {
    assert.is_true(baml.deep_equals("abc".is_ascii_graphic(), true))
}

test "string_is_ascii_graphic_punctuation" {
    assert.is_true(baml.deep_equals("!@#~".is_ascii_graphic(), true))
}

// ASCII space is NOT graphic.
test "string_is_ascii_graphic_space_false" {
    assert.is_true(baml.deep_equals(" ".is_ascii_graphic(), false))
}

test "string_is_ascii_graphic_control_false" {
    assert.is_true(baml.deep_equals("\n".is_ascii_graphic(), false))
}

// é is not ASCII graphic (non-ASCII).
test "string_is_ascii_graphic_accented_false" {
    assert.is_true(baml.deep_equals("é".is_ascii_graphic(), false))
}

test "string_is_ascii_graphic_empty_true" {
    assert.is_true(baml.deep_equals("".is_ascii_graphic(), true))
}

// ── is_ascii_hex ──

test "string_is_ascii_hex_lower" {
    assert.is_true(baml.deep_equals("deadbeef".is_ascii_hex(), true))
}

test "string_is_ascii_hex_upper" {
    assert.is_true(baml.deep_equals("DEADBEEF".is_ascii_hex(), true))
}

test "string_is_ascii_hex_mixed_case_with_digits" {
    assert.is_true(baml.deep_equals("00aF99".is_ascii_hex(), true))
}

test "string_is_ascii_hex_g_false" {
    assert.is_true(baml.deep_equals("0g".is_ascii_hex(), false))
}

test "string_is_ascii_hex_x_prefix_false" {
    assert.is_true(baml.deep_equals("0x123".is_ascii_hex(), false))
}

test "string_is_ascii_hex_empty_true" {
    assert.is_true(baml.deep_equals("".is_ascii_hex(), true))
}

// ─── replace ─────────────────────────────────────────────────────────────────

// replace replaces only the first occurrence.
test "string_replace" {
    assert.is_true(baml.deep_equals("hello world world".replace("world", "BAML"), "hello BAML world"))
}
