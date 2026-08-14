//! Repro for a reported SAP bug: an unquoted Cyrillic sentence value with a
//! trailing `.` (period) inside an object fails to parse, while the same input
//! with that one value quoted parses fine.
//!
//! Schema under test (annotations that don't affect coercion of these inputs —
//! block attributes and `@description` are omitted):
//!
//! ```baml
//! class Word {
//!     word string
//!     definitions Definition[] @alias("translations")
//! }
//! class Definition {
//!     definition string @alias("translation")
//!     partOfSpeech string
//!     exampleSentence string
//!     exampleTranslation string @alias("exampleSentenceTranslation")
//! }
//! ```

use crate::{baml_db, baml_tyannotated};

// The `exampleSentence` value is QUOTED. The user reports this parses.
test_deserializer!(
    test_word_quoted_example_sentence,
    r#"{
  word: фрукт,
  translations: [
    {
      translation: fruit,
      partOfSpeech: noun,
      exampleSentence: "Я люблю есть фрукт после обеда.",
      exampleSentenceTranslation: I like to eat fruit after lunch,
    }
  ],
}"#,
    baml_tyannotated!(Word),
    baml_db! {
        class Definition {
            definition: string @alias("translation"),
            partOfSpeech: string,
            exampleSentence: string,
            exampleTranslation: string @alias("exampleSentenceTranslation"),
        }
        class Word {
            word: string,
            definitions: [Definition] @alias("translations"),
        }
    },
    {
        "word": "фрукт",
        "definitions": [
            {
                "definition": "fruit",
                "partOfSpeech": "noun",
                "exampleSentence": "Я люблю есть фрукт после обеда.",
                "exampleTranslation": "I like to eat fruit after lunch"
            }
        ]
    }
);

// ===========================================================================
// Factor isolation. The two suspects are (a) non-ASCII / Cyrillic, and
// (b) a trailing `.` immediately before the `,` delimiter. The macro below
// keeps everything constant except the unquoted `exampleSentence` value.
// ===========================================================================

macro_rules! word_unquoted_example {
    ($name:ident, $sentence:literal, $expected:literal) => {
        test_deserializer!(
            $name,
            concat!(
                "{\n  word: фрукт,\n  translations: [\n    {\n",
                "      translation: fruit,\n      partOfSpeech: noun,\n",
                "      exampleSentence: ", $sentence, ",\n",
                "      exampleSentenceTranslation: I like to eat fruit after lunch,\n",
                "    }\n  ],\n}"
            ),
            baml_tyannotated!(Word),
            baml_db! {
                class Definition {
                    definition: string @alias("translation"),
                    partOfSpeech: string,
                    exampleSentence: string,
                    exampleTranslation: string @alias("exampleSentenceTranslation"),
                }
                class Word {
                    word: string,
                    definitions: [Definition] @alias("translations"),
                }
            },
            {
                "word": "фрукт",
                "definitions": [
                    {
                        "definition": "fruit",
                        "partOfSpeech": "noun",
                        "exampleSentence": $expected,
                        "exampleTranslation": "I like to eat fruit after lunch"
                    }
                ]
            }
        );
    };
}

// (a) Cyrillic, WITH trailing period  -> the original failing case.
word_unquoted_example!(
    test_factor_cyrillic_with_period,
    "Я люблю есть фрукт после обеда.",
    "Я люблю есть фрукт после обеда."
);

// (b) Cyrillic, WITHOUT trailing period -> isolates the period factor.
word_unquoted_example!(
    test_factor_cyrillic_no_period,
    "Я люблю есть фрукт после обеда",
    "Я люблю есть фрукт после обеда"
);

// (c) ASCII, WITH trailing period -> isolates the non-ASCII factor.
word_unquoted_example!(
    test_factor_ascii_with_period,
    "I love eating fruit after lunch.",
    "I love eating fruit after lunch."
);

// (d) ASCII, WITHOUT trailing period -> control (expected to pass).
word_unquoted_example!(
    test_factor_ascii_no_period,
    "I love eating fruit after lunch",
    "I love eating fruit after lunch"
);

// (e) Cyrillic with an INTERNAL period but not trailing -> does the period
// only matter when adjacent to the `,` delimiter?
word_unquoted_example!(
    test_factor_cyrillic_internal_period,
    "Я люблю. есть фрукт после обеда",
    "Я люблю. есть фрукт после обеда"
);

// (f) Single-word Cyrillic (NO spaces) -> the unquoted `word: фрукт` field
// parses fine, so does a single Cyrillic token work in the nested position?
word_unquoted_example!(test_factor_cyrillic_single_word, "фрукт", "фрукт");

// (g) Two-word Cyrillic -> minimal "spaces + non-ASCII" case.
word_unquoted_example!(test_factor_cyrillic_two_words, "Я люблю", "Я люблю");

// (h) Single multi-byte non-ASCII char, no spaces (Greek) -> non-ASCII alone.
word_unquoted_example!(test_factor_greek_single_word, "καλημέρα", "καλημέρα");

// Bisect the word-count threshold for unquoted Cyrillic (2 words passes,
// 6 words fails).
word_unquoted_example!(test_factor_cyrillic_3_words, "Я люблю есть", "Я люблю есть");
word_unquoted_example!(
    test_factor_cyrillic_4_words,
    "Я люблю есть фрукт",
    "Я люблю есть фрукт"
);
word_unquoted_example!(
    test_factor_cyrillic_5_words,
    "Я люблю есть фрукт после",
    "Я люблю есть фрукт после"
);

// Same word counts in ASCII, as a parallel control.
word_unquoted_example!(test_factor_ascii_3_words, "a b c", "a b c");
word_unquoted_example!(test_factor_ascii_6_words, "a b c d e f", "a b c d e f");

// Long single Cyrillic token (no spaces) but many bytes -> length vs words.
word_unquoted_example!(
    test_factor_cyrillic_long_single_word,
    "ялюблюестьфруктпослеобеда",
    "ялюблюестьфруктпослеобеда"
);

// Precise byte-length bisection: pure Cyrillic single tokens, each letter is
// 2 UTF-8 bytes. Name encodes letter count / byte count.
word_unquoted_example!(test_bytes_06l_12b, "абвгде", "абвгде"); // 12 bytes
word_unquoted_example!(test_bytes_07l_14b, "абвгдеж", "абвгдеж"); // 14 bytes
word_unquoted_example!(test_bytes_08l_16b, "абвгдежз", "абвгдежз"); // 16 bytes
word_unquoted_example!(test_bytes_09l_18b, "абвгдежзи", "абвгдежзи"); // 18 bytes
word_unquoted_example!(test_bytes_10l_20b, "абвгдежзик", "абвгдежзик"); // 20 bytes

// Disambiguate char-count vs byte-count vs ASCII-exemption.
// 10 chars but only 19 bytes (9 Cyrillic + 1 ASCII): char>=10 fails, byte>=20 passes.
word_unquoted_example!(test_disambig_10chars_19bytes, "абвгдежзиx", "абвгдежзиx");
// 20-char / 20-byte pure-ASCII token: confirms ASCII is exempt at this length.
word_unquoted_example!(
    test_disambig_ascii_20chars,
    "abcdefghijklmnopqrst",
    "abcdefghijklmnopqrst"
);
// 11 chars / 22 bytes pure Cyrillic, just past the boundary.
word_unquoted_example!(test_disambig_11l_22b, "абвгдежзикл", "абвгдежзикл");

// The `exampleSentence` value is UNQUOTED (the ONLY difference from the test
// above). The user reports this does NOT parse — so we assert the *expected*
// (correct) result; if the bug is real this test will fail.
test_deserializer!(
    test_word_unquoted_example_sentence,
    r#"{
  word: фрукт,
  translations: [
    {
      translation: fruit,
      partOfSpeech: noun,
      exampleSentence: Я люблю есть фрукт после обеда.,
      exampleSentenceTranslation: I like to eat fruit after lunch,
    }
  ],
}"#,
    baml_tyannotated!(Word),
    baml_db! {
        class Definition {
            definition: string @alias("translation"),
            partOfSpeech: string,
            exampleSentence: string,
            exampleTranslation: string @alias("exampleSentenceTranslation"),
        }
        class Word {
            word: string,
            definitions: [Definition] @alias("translations"),
        }
    },
    {
        "word": "фрукт",
        "definitions": [
            {
                "definition": "fruit",
                "partOfSpeech": "noun",
                "exampleSentence": "Я люблю есть фрукт после обеда.",
                "exampleTranslation": "I like to eat fruit after lunch"
            }
        ]
    }
);

// The EXACT raw output from the reported Discord thread (note both unquoted
// sentences end with ".,"). Regression test for the real-world report.
test_deserializer!(
    test_word_exact_discord_report,
    "{\n  word: фрукт,\n  translations: [\n    {\n      translation: fruit,\n      partOfSpeech: noun,\n      exampleSentence: Я люблю есть фрукт после обеда.,\n      exampleSentenceTranslation: I like to eat fruit after lunch.,\n    }\n  ],\n}",
    baml_tyannotated!(Word),
    baml_db! {
        class Definition {
            definition: string @alias("translation"),
            partOfSpeech: string,
            exampleSentence: string,
            exampleTranslation: string @alias("exampleSentenceTranslation"),
        }
        class Word {
            word: string,
            definitions: [Definition] @alias("translations"),
        }
    },
    {
        "word": "фрукт",
        "definitions": [
            {
                "definition": "fruit",
                "partOfSpeech": "noun",
                "exampleSentence": "Я люблю есть фрукт после обеда.",
                "exampleTranslation": "I like to eat fruit after lunch."
            }
        ]
    }
);
