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
    assert.equal(strings_concat(), "Hello World")
}

// ─── comparisons ─────────────────────────────────────────────────────────────

test "string_equality_true" {
    assert.is_true("Hello" == "Hello")
}

test "string_equality_false" {
    assert.is_true("Hello" != "World")
}

test "string_not_equal_true" {
    assert.is_true("Hello" != "World")
}

test "string_less_than" {
    assert.is_true("a" < "b")
}

test "string_less_than_or_equal" {
    assert.is_true("a" <= "b")
}

test "string_greater_than" {
    assert.is_true("b" > "a")
}

test "string_greater_than_or_equal" {
    assert.is_true("b" >= "a")
}

// ─── basic methods ────────────────────────────────────────────────────────────

test "string_length" {
    assert.equal("hello".length(), 5)
}

test "string_to_lower_case" {
    assert.equal("HELLO World".to_lower_case(), "hello world")
}

test "string_to_upper_case" {
    assert.equal("hello WORLD".to_upper_case(), "HELLO WORLD")
}

test "string_trim" {
    assert.equal("  hello world  ".trim(), "hello world")
}

test "string_includes" {
    assert.is_true("hello world".includes("world"))
}

test "string_starts_with" {
    assert.is_true("hello world".starts_with("hello"))
}

test "string_ends_with" {
    assert.is_true("hello world".ends_with("world"))
}

test "string_split" {
    assert.equal("hello,world,test".split(","), ["hello", "world", "test"])
}

test "string_slice" {
    assert.equal("hello world".slice(0, 5), "hello")
}

test "string_slice_bounds" {
    // end byte offset clamped to length
    assert.equal("hello".slice(2, 10), "llo")
}

// ─── codepoint-indexed semantics ─────────────────────────────────────────────

// length() returns codepoints, not bytes.
test "string_length_codepoints_non_ascii" {
    assert.equal("héllo".length(), 5)
}

test "string_length_codepoints_emoji" {
    assert.equal("🐈".length(), 1)
}

// byte_length() returns UTF-8 bytes.
test "string_byte_length_non_ascii" {
    assert.equal("héllo".byte_length(), 6)
}

test "string_byte_length_emoji" {
    assert.equal("🐈".byte_length(), 4)
}

// slice uses codepoint indices.
test "string_slice_codepoint_indexed" {
    assert.equal("héllo".slice(0, 2), "hé")
}

test "string_slice_emoji_first_char" {
    assert.equal("😀hello".slice(0, 1), "😀")
}

test "string_slice_after_emoji" {
    assert.equal("😀hello".slice(1, 4), "hel")
}

// Negative indices count from the end (length 5): -3 → 2, -1 → 4.
test "string_slice_negative_counts_from_end" {
    assert.equal("hello".slice(-3, 4), "ll");
    assert.equal("hello".slice(-3, -1), "ll")
}

// A negative index past the start clamps to 0.
test "string_slice_negative_clamps_to_zero" {
    assert.equal("hello".slice(-100, 4), "hell")
}

test "string_slice_empty_when_start_past_end" {
    assert.equal("hello".slice(4, 2), "")
}

test "string_slice_out_of_bounds_clamps" {
    assert.equal("hello".slice(0, 100), "hello")
}

// at uses codepoint index.
test "string_at_multibyte" {
    assert.equal("héllo".at(1), "é")
}

test "string_at_after_multibyte" {
    assert.equal("héllo".at(2), "l")
}

test "string_at_emoji" {
    assert.equal("😀hello".at(0), "😀")
}

test "string_at_after_emoji" {
    assert.equal("😀hello".at(1), "h")
}

// at at or past the end returns null ("hello" has length 5).
test "string_at_at_length_is_null" {
    assert.equal("hello".at(5), null)
}

test "string_at_past_end_is_null" {
    assert.equal("hello".at(99), null)
}

// Negative indices count from the end: -1 is the last character.
test "string_at_negative_counts_from_end" {
    assert.equal("hello".at(-1), "o");
    // "héllo" = h é l l o; -4 normalizes to codepoint 1.
    assert.equal("héllo".at(-4), "é")
}

// Past the start (normalizes before index 0) also returns null.
test "string_at_past_start_is_null" {
    assert.equal("hello".at(-6), null)
}

// The empty string has no character at any index.
test "string_at_empty_is_null" {
    assert.equal("".at(0), null);
    assert.equal("".at(-1), null)
}

// The optional result compares directly against a literal — an out-of-range
// null is simply unequal to every character, so scanning loops need no unwrap.
test "string_at_compares_against_literal" {
    assert.is_true("hi".at(0) == "h");
    assert.is_true("hi".at(9) != "h")
}

// ─── code_point_at ─────────────────────────────────────────────────────────────

// code_point_at is the numeric counterpart of at: same codepoint indexing and
// the same null-when-out-of-range rule, but returns the code point's value
// instead of a one-character string.
test "string_code_point_at_ascii" {
    assert.equal("A".code_point_at(0), 65)
}

test "string_code_point_at_multibyte" {
    // "héllo" = h é l l o; index 1 is é (U+00E9 = 233).
    assert.equal("héllo".code_point_at(1), 233)
}

test "string_code_point_at_emoji" {
    // A single emoji is one codepoint, not a pair of UTF-16 units.
    assert.equal("🐑hello".code_point_at(0), 128017)
}

test "string_code_point_at_after_emoji" {
    assert.equal("😀hello".code_point_at(1), 104)
}

// Negative indices count from the end: -1 is the last character.
test "string_code_point_at_negative_counts_from_end" {
    assert.equal("hello".code_point_at(-1), 111);
    assert.equal("héllo".code_point_at(-4), 233)
}

// code_point_at at or past the end returns null ("hello" has length 5).
test "string_code_point_at_at_length_is_null" {
    assert.equal("hello".code_point_at(5), null)
}

test "string_code_point_at_past_end_is_null" {
    assert.equal("hello".code_point_at(99), null)
}

// Past the start (normalizes before index 0) also returns null.
test "string_code_point_at_past_start_is_null" {
    assert.equal("hello".code_point_at(-6), null)
}

// `at` and `code_point_at` agree on which indices are in range.
test "string_at_and_code_point_at_agree_on_range" {
    assert.equal("hi".at(2), null);
    assert.equal("hi".code_point_at(2), null)
}

// Every in-range code point round-trips through from_code_points.
test "string_code_point_at_round_trips_through_from_code_points" {
    if let cp: int = ("é").code_point_at(0) {
        assert.equal(string.from_code_points([cp]), "é")
    } else {
        baml.sys.panic("code_point_at(0) of a one-character string is never null")
    }
}

// index_of returns codepoint index.
test "string_index_of_returns_codepoint_index" {
    assert.equal("héllo".index_of("l"), 2)
}

test "string_index_of_after_emoji" {
    assert.equal("😀hello".index_of("h"), 1)
}

test "string_index_of_absent_is_null" {
    assert.equal("abc".index_of("z"), null)
}

// ─── last_index_of ───────────────────────────────────────────────────────────

// Like index_of, but scans from the end; the reported index is still the
// codepoint offset of the match's *start*.
test "string_last_index_of_returns_last_match" {
    assert.equal("hello world".last_index_of("o"), 7)
}

test "string_last_index_of_multi_character_needle" {
    assert.equal("abcabc".last_index_of("bc"), 4)
}

// With exactly one occurrence the two agree.
test "string_last_index_of_agrees_with_index_of_when_unique" {
    assert.equal("hello world".last_index_of("world"), 6);
    assert.equal("hello world".index_of("world"), 6)
}

test "string_last_index_of_codepoint_indexed" {
    assert.equal("héllo".last_index_of("l"), 3)
}

test "string_last_index_of_after_emoji" {
    assert.equal("😀hello".last_index_of("h"), 1)
}

test "string_last_index_of_absent_is_null" {
    assert.equal("abc".last_index_of("z"), null)
}

// An empty needle matches at the end, mirroring index_of matching at the start.
test "string_last_index_of_empty_needle" {
    assert.equal("abc".index_of(""), 0);
    assert.equal("abc".last_index_of(""), 3)
}

// ─── repeat ──────────────────────────────────────────────────────────────────

test "string_repeat" {
    assert.equal("ab".repeat(3), "ababab")
}

test "string_repeat_zero" {
    assert.equal("ab".repeat(0), "")
}

test "string_repeat_negative" {
    assert.equal("ab".repeat(-1), "")
}

// ─── char_count ───────────────────────────────────────────────────────────────

test "string_char_count_ascii" {
    assert.equal("hello".char_count(), 5)
}

test "string_char_count_multibyte" {
    assert.equal("héllo".char_count(), 5)
}

test "string_char_count_emoji" {
    assert.equal("🐑🐑".char_count(), 2)
}

test "string_char_count_empty" {
    assert.equal("".char_count(), 0)
}

// ─── trim_start / trim_end ────────────────────────────────────────────────────

test "string_trim_start_removes_leading_only" {
    assert.equal("  hi  ".trim_start(), "hi  ")
}

test "string_trim_end_removes_trailing_only" {
    assert.equal("  hi  ".trim_end(), "  hi")
}

test "string_trim_start_handles_newlines_and_tabs" {
    assert.equal("\n\t hi".trim_start(), "hi")
}

test "string_trim_end_no_whitespace_unchanged" {
    assert.equal("hi".trim_end(), "hi")
}

test "string_trim_start_empty" {
    assert.equal("".trim_start(), "")
}

// ─── lines ────────────────────────────────────────────────────────────────────

test "string_lines_lf" {
    assert.equal("a\nb\nc".lines(), ["a", "b", "c"])
}

test "string_lines_crlf" {
    assert.equal("a\r\nb\r\nc".lines(), ["a", "b", "c"])
}

// Trailing newline does not add an empty entry.
test "string_lines_trailing_newline_no_empty" {
    assert.equal("a\nb\n".lines(), ["a", "b"])
}

test "string_lines_empty_string" {
    // "".lines() is a call result; compare its length instead of against an
    // empty-array local (which hits the VM boxing bug).
    assert.equal("".lines().length(), 0)
}

// A single "\n" produces one empty string (the line before the newline).
test "string_lines_just_newline" {
    assert.equal("\n".lines(), [""])
}

// ─── to_utf8 / from_utf8 ──────────────────────────────────────────────────────

// "hi" → [0x68, 0x69]
test "string_to_utf8_ascii" {
    assert.equal("hi".to_utf8(), b"\x68\x69")
}

// "é" (U+00E9) → [0xC3, 0xA9] in UTF-8
test "string_to_utf8_multibyte" {
    assert.equal("é".to_utf8(), b"\xC3\xA9")
}

test "string_to_utf8_empty" {
    assert.equal("".to_utf8(), b"")
}

test "string_from_utf8_ascii_round_trip" {
    assert.equal(string.from_utf8(b"\x68\x69"), "hi")
}

test "string_from_utf8_multibyte_round_trip" {
    assert.equal(string.from_utf8(b"\xC3\xA9"), "é")
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
    assert.equal(string.from_utf8(b""), "")
}

// ─── to_code_points ───────────────────────────────────────────────────────────

// to_code_points is the exact inverse of from_code_points: one int per character.
test "string_to_code_points_ascii" {
    assert.equal("hi".to_code_points(), [104, 105])
}

test "string_to_code_points_multibyte" {
    // "héllo" = h é l l o; é is U+00E9 = 233.
    assert.equal("héllo".to_code_points(), [104, 233, 108, 108, 111])
}

test "string_to_code_points_emoji" {
    // A single emoji decodes to one codepoint, not a UTF-16 surrogate pair.
    assert.equal("🐑".to_code_points(), [128017])
}

test "string_to_code_points_empty" {
    assert.equal("".to_code_points(), [])
}

// from_code_points ∘ to_code_points is the identity over arbitrary text.
test "string_to_code_points_round_trips" {
    let s = "Hëllo 🐑 World";
    assert.equal(string.from_code_points(s.to_code_points()), s)
}

// ─── from_code_points ─────────────────────────────────────────────────────────

test "string_from_code_points_ascii" {
    assert.equal(string.from_code_points([104, 105]), "hi")
}

test "string_from_code_points_multibyte" {
    // 233 = U+00E9 = é
    assert.equal(string.from_code_points([233]), "é")
}

test "string_from_code_points_emoji" {
    // 128017 = U+1F411 = 🐑
    assert.equal(string.from_code_points([128017]), "🐑")
}

test "string_from_code_points_empty" {
    assert.equal(string.from_code_points([]), "")
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
    assert.is_true("12345".is_numeric())
}

// U+2167 ROMAN NUMERAL EIGHT — Unicode general category Nl.
test "string_is_numeric_unicode_roman" {
    assert.is_true("Ⅷ".is_numeric())
}

test "string_is_numeric_mixed_false" {
    assert.is_true(!"12a".is_numeric())
}

test "string_is_numeric_letters_false" {
    assert.is_true(!"abc".is_numeric())
}

test "string_is_numeric_empty_true" {
    assert.is_true("".is_numeric())
}

// ── is_alphabetic (Unicode) ──

test "string_is_alphabetic_ascii" {
    assert.is_true("hello".is_alphabetic())
}

test "string_is_alphabetic_accented" {
    assert.is_true("héllo".is_alphabetic())
}

// 漢字 — common CJK letters.
test "string_is_alphabetic_cjk" {
    assert.is_true("漢字".is_alphabetic())
}

test "string_is_alphabetic_with_digit_false" {
    assert.is_true(!"abc1".is_alphabetic())
}

test "string_is_alphabetic_with_space_false" {
    assert.is_true(!"a b".is_alphabetic())
}

test "string_is_alphabetic_empty_true" {
    assert.is_true("".is_alphabetic())
}

// ── is_alphanumeric (Unicode) ──

test "string_is_alphanumeric_letters_and_digits" {
    assert.is_true("abc123".is_alphanumeric())
}

test "string_is_alphanumeric_accented_with_digit" {
    assert.is_true("héllo7".is_alphanumeric())
}

test "string_is_alphanumeric_with_space_false" {
    assert.is_true(!"a b".is_alphanumeric())
}

test "string_is_alphanumeric_with_punct_false" {
    assert.is_true(!"a!b".is_alphanumeric())
}

test "string_is_alphanumeric_empty_true" {
    assert.is_true("".is_alphanumeric())
}

// ── is_uppercase / is_lowercase (Unicode) ──

test "string_is_uppercase_all_upper" {
    assert.is_true("HELLO".is_uppercase())
}

test "string_is_uppercase_mixed_false" {
    assert.is_true(!"Hello".is_uppercase())
}

// Digits are not uppercase.
test "string_is_uppercase_digit_false" {
    assert.is_true(!"1".is_uppercase())
}

test "string_is_uppercase_empty_true" {
    assert.is_true("".is_uppercase())
}

test "string_is_lowercase_all_lower" {
    assert.is_true("hello".is_lowercase())
}

test "string_is_lowercase_mixed_false" {
    assert.is_true(!"Hello".is_lowercase())
}

test "string_is_lowercase_accented_lower" {
    assert.is_true("héllo".is_lowercase())
}

test "string_is_lowercase_empty_true" {
    assert.is_true("".is_lowercase())
}

// ── is_whitespace (Unicode) ──

test "string_is_whitespace_spaces" {
    assert.is_true("   ".is_whitespace())
}

test "string_is_whitespace_mixed_ws" {
    assert.is_true(" \t\n\r".is_whitespace())
}

// U+00A0 NO-BREAK SPACE — Unicode whitespace. BAML strings don't support
// \u{...} escapes, so we build the character from its UTF-8 bytes.
test "string_is_whitespace_unicode_nbsp" {
    assert.is_true(strings_nbsp().is_whitespace())
}

test "string_is_whitespace_with_letter_false" {
    assert.is_true(!"a b".is_whitespace())
}

test "string_is_whitespace_empty_true" {
    assert.is_true("".is_whitespace())
}

// ── is_control / is_graphic ──

test "string_is_control_newline_tab" {
    assert.is_true("\n\t".is_control())
}

test "string_is_control_letter_false" {
    assert.is_true(!"a".is_control())
}

test "string_is_control_empty_true" {
    assert.is_true("".is_control())
}

test "string_is_graphic_letters" {
    assert.is_true("abc".is_graphic())
}

test "string_is_graphic_accented" {
    assert.is_true("héllo".is_graphic())
}

test "string_is_graphic_with_space_false" {
    assert.is_true(!"a b".is_graphic())
}

test "string_is_graphic_with_newline_false" {
    assert.is_true(!"a\n".is_graphic())
}

test "string_is_graphic_empty_true" {
    assert.is_true("".is_graphic())
}

// ── is_ascii ──

test "string_is_ascii_pure_ascii" {
    assert.is_true("hello world!".is_ascii())
}

test "string_is_ascii_with_accent_false" {
    assert.is_true(!"héllo".is_ascii())
}

test "string_is_ascii_with_emoji_false" {
    assert.is_true(!"🐑".is_ascii())
}

// \x00 (\0) and \x7F (DEL) are both valid ASCII bytes.
// \x7F is not a BAML string escape, so we build the test string via bytes.
function strings_nul_and_del() -> string {
    string.from_utf8(b"\x00\x7F")
}

test "string_is_ascii_control_chars" {
    assert.is_true(strings_nul_and_del().is_ascii())
}

test "string_is_ascii_empty_true" {
    assert.is_true("".is_ascii())
}

// ── is_ascii_numeric ──

test "string_is_ascii_numeric_digits" {
    assert.is_true("01234".is_ascii_numeric())
}

// Unicode numeral Ⅷ is rejected by the ASCII variant.
test "string_is_ascii_numeric_unicode_numeral_false" {
    assert.is_true(!"Ⅷ".is_ascii_numeric())
}

test "string_is_ascii_numeric_letter_false" {
    assert.is_true(!"1a".is_ascii_numeric())
}

test "string_is_ascii_numeric_empty_true" {
    assert.is_true("".is_ascii_numeric())
}

// ── is_ascii_alphabetic / is_ascii_alphanumeric ──

test "string_is_ascii_alphabetic_letters" {
    assert.is_true("Hello".is_ascii_alphabetic())
}

test "string_is_ascii_alphabetic_accented_false" {
    assert.is_true(!"héllo".is_ascii_alphabetic())
}

test "string_is_ascii_alphabetic_with_digit_false" {
    assert.is_true(!"abc1".is_ascii_alphabetic())
}

test "string_is_ascii_alphabetic_empty_true" {
    assert.is_true("".is_ascii_alphabetic())
}

test "string_is_ascii_alphanumeric_basic" {
    assert.is_true("abc123".is_ascii_alphanumeric())
}

test "string_is_ascii_alphanumeric_accented_false" {
    assert.is_true(!"héllo1".is_ascii_alphanumeric())
}

test "string_is_ascii_alphanumeric_with_space_false" {
    assert.is_true(!"a 1".is_ascii_alphanumeric())
}

test "string_is_ascii_alphanumeric_empty_true" {
    assert.is_true("".is_ascii_alphanumeric())
}

// ── is_ascii_uppercase / is_ascii_lowercase ──

test "string_is_ascii_uppercase_pure_upper" {
    assert.is_true("ABC".is_ascii_uppercase())
}

test "string_is_ascii_uppercase_mixed_false" {
    assert.is_true(!"Abc".is_ascii_uppercase())
}

// É (U+00C9) is upper but not ASCII.
test "string_is_ascii_uppercase_with_accent_false" {
    assert.is_true(!"É".is_ascii_uppercase())
}

test "string_is_ascii_uppercase_empty_true" {
    assert.is_true("".is_ascii_uppercase())
}

test "string_is_ascii_lowercase_pure_lower" {
    assert.is_true("abc".is_ascii_lowercase())
}

test "string_is_ascii_lowercase_mixed_false" {
    assert.is_true(!"aBc".is_ascii_lowercase())
}

// é is lower but not ASCII.
test "string_is_ascii_lowercase_with_accent_false" {
    assert.is_true(!"héllo".is_ascii_lowercase())
}

test "string_is_ascii_lowercase_empty_true" {
    assert.is_true("".is_ascii_lowercase())
}

// ── is_ascii_whitespace ──

test "string_is_ascii_whitespace_basic" {
    assert.is_true(" \t\n\r".is_ascii_whitespace())
}

// Form feed \x0C is ASCII whitespace. BAML strings don't support \x escapes,
// so we decode from bytes.
test "string_is_ascii_whitespace_form_feed" {
    assert.is_true(strings_form_feed().is_ascii_whitespace())
}

// NBSP (U+00A0) is whitespace per Unicode but not per ASCII.
test "string_is_ascii_whitespace_nbsp_false" {
    assert.is_true(!strings_nbsp().is_ascii_whitespace())
}

test "string_is_ascii_whitespace_letter_false" {
    assert.is_true(!"a".is_ascii_whitespace())
}

test "string_is_ascii_whitespace_empty_true" {
    assert.is_true("".is_ascii_whitespace())
}

// ── is_ascii_control ──

// \x01 and \x1F are not representable as BAML string escapes; build via bytes.
test "string_is_ascii_control_low_range" {
    assert.is_true(strings_low_controls().is_ascii_control())
}

// DEL (0x7F) is ASCII control.
test "string_is_ascii_control_del" {
    assert.is_true(strings_del().is_ascii_control())
}

test "string_is_ascii_control_letter_false" {
    assert.is_true(!"a".is_ascii_control())
}

test "string_is_ascii_control_empty_true" {
    assert.is_true("".is_ascii_control())
}

// ── is_ascii_graphic ──

test "string_is_ascii_graphic_letters" {
    assert.is_true("abc".is_ascii_graphic())
}

test "string_is_ascii_graphic_punctuation" {
    assert.is_true("!@#~".is_ascii_graphic())
}

// ASCII space is NOT graphic.
test "string_is_ascii_graphic_space_false" {
    assert.is_true(!" ".is_ascii_graphic())
}

test "string_is_ascii_graphic_control_false" {
    assert.is_true(!"\n".is_ascii_graphic())
}

// é is not ASCII graphic (non-ASCII).
test "string_is_ascii_graphic_accented_false" {
    assert.is_true(!"é".is_ascii_graphic())
}

test "string_is_ascii_graphic_empty_true" {
    assert.is_true("".is_ascii_graphic())
}

// ── is_ascii_hex ──

test "string_is_ascii_hex_lower" {
    assert.is_true("deadbeef".is_ascii_hex())
}

test "string_is_ascii_hex_upper" {
    assert.is_true("DEADBEEF".is_ascii_hex())
}

test "string_is_ascii_hex_mixed_case_with_digits" {
    assert.is_true("00aF99".is_ascii_hex())
}

test "string_is_ascii_hex_g_false" {
    assert.is_true(!"0g".is_ascii_hex())
}

test "string_is_ascii_hex_x_prefix_false" {
    assert.is_true(!"0x123".is_ascii_hex())
}

test "string_is_ascii_hex_empty_true" {
    assert.is_true("".is_ascii_hex())
}

// ─── replace ─────────────────────────────────────────────────────────────────

// replace replaces only the first occurrence.
test "string_replace" {
    assert.equal("hello world world".replace("world", "BAML"), "hello BAML world")
}
