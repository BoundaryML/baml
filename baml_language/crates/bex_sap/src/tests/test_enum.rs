use crate::{baml_db, baml_tyannotated};

test_deserializer!(
    test_enum,
    r#"TWO"#,
    baml_tyannotated!(Category),
    baml_db! {
        enum Category {
            ONE,
            TWO
        }
    },
    "TWO"
);

test_deserializer!(
    case_insensitive,
    r#"two"#,
    baml_tyannotated!(Category),
    baml_db! {
        enum Category {
            ONE,
            TWO
        }
    },
    "TWO"
);

test_deserializer!(
    with_quotes,
    r#""TWO""#,
    baml_tyannotated!(Category),
    baml_db! {
        enum Category {
            ONE,
            TWO
        }
    },
    "TWO"
);

test_deserializer!(
    from_enum_list_single,
    r#"["TWO"]"#,
    baml_tyannotated!(Category),
    baml_db! {
        enum Category {
            ONE,
            TWO
        }
    },
    "TWO"
);

test_deserializer!(
    from_enum_list_multi,
    r#"["TWO", "THREE"]"#,
    baml_tyannotated!(Category),
    baml_db! {
        enum Category {
            ONE,
            TWO
        }
    },
    "TWO"
);

test_deserializer!(
    from_string_with_extra_text_after_1,
    r#""ONE: The description of k1""#,
    baml_tyannotated!(Category),
    baml_db! {
        enum Category {
            ONE,
            TWO
        }
    },
    "ONE"
);

test_deserializer!(
    from_string_and_case_mismatch,
    "The answer is One",
    baml_tyannotated!(Category),
    baml_db! {
        enum Category {
            ONE,
            TWO
        }
    },
    "ONE"
);

test_deserializer!(
    from_string_and_case_mismatch_wrapped,
    "**one** is the answer",
    baml_tyannotated!(Category),
    baml_db! {
        enum Category {
            ONE,
            TWO
        }
    },
    "ONE"
);

test_deserializer!(
    from_string_and_case_mismatch_upper,
    "**ONE** is the answer",
    baml_tyannotated!(PascalCaseCategory),
    baml_db! {
        enum PascalCaseCategory {
            One,
            Two
        }
    },
    "One"
);

test_deserializer!(
    from_string_with_extra_text_after_2,
    r#""ONE - The description of an enum value""#,
    baml_tyannotated!(Category),
    baml_db! {
        enum Category {
            ONE,
            TWO
        }
    },
    "ONE"
);

test_deserializer!(
    case_sensitive_non_ambiguous_match,
    r#"TWO" is one of the correct answers."#,
    baml_tyannotated!(Category),
    baml_db! {
        enum Category {
            ONE,
            TWO
        }
    },
    "TWO"
);

test_failing_deserializer!(
    case_insensitive_ambiguous_match,
    r#"Two" is one of the correct answers."#,
    baml_tyannotated!(Category),
    baml_db! {
        enum Category {
            ONE,
            TWO
        }
    }
);

test_failing_deserializer!(
    from_string_with_extra_text_after_3,
    r#""ONE - is the answer, not TWO""#,
    baml_tyannotated!(Category),
    baml_db! {
        enum Category {
            ONE,
            TWO
        }
    }
);

test_failing_deserializer!(
    from_string_with_extra_text_after_4,
    r#""ONE. is the answer, not TWO""#,
    baml_tyannotated!(Category),
    baml_db! {
        enum Category {
            ONE,
            TWO
        }
    }
);

test_failing_deserializer!(
    from_string_with_extra_text_after_5,
    r#""ONE: is the answer, not TWO""#,
    baml_tyannotated!(Category),
    baml_db! {
        enum Category {
            ONE,
            TWO
        }
    }
);

test_deserializer!(
    aliases_1,
    r#"k1"#,
    baml_tyannotated!(Category),
    baml_db! {
        enum Category {
            ONE @alias("k1"),
            TWO @alias("k-2-3.1_1"),
            THREE @alias("NUMBER THREE")
        }
    },
    "ONE"
);

test_deserializer!(
    aliases_2,
    r#"k-2-3.1_1"#,
    baml_tyannotated!(Category),
    baml_db! {
        enum Category {
            ONE @alias("k1"),
            TWO @alias("k-2-3.1_1"),
            THREE @alias("NUMBER THREE")
        }
    },
    "TWO"
);

test_deserializer!(
    aliases_3,
    r#"NUMBER THREE"#,
    baml_tyannotated!(Category),
    baml_db! {
        enum Category {
            ONE @alias("k1"),
            TWO @alias("k-2-3.1_1"),
            THREE @alias("NUMBER THREE")
        }
    },
    "THREE"
);

test_deserializer!(
    no_punctuation,
    r#"number three"#,
    baml_tyannotated!(Category),
    baml_db! {
        enum Category {
            ONE @alias("k1"),
            TWO @alias("k-2-3.1_1"),
            THREE @alias("NUMBER THREE")
        }
    },
    "THREE"
);

test_deserializer!(
    no_punctuation_2,
    r#"k-2-3 1_1"#,
    baml_tyannotated!(Category),
    baml_db! {
        enum Category {
            ONE @alias("k1"),
            TWO @alias("k-2-3.1_1"),
            THREE @alias("NUMBER THREE")
        }
    },
    "TWO"
);

test_deserializer!(
    descriptions,
    r#"k1: The description of enum value une"#,
    baml_tyannotated!(Category),
    baml_db! {
        enum Category {
            ONE @alias("k1"),
            TWO @alias("k-2-3.1_1"),
            THREE @alias("NUMBER THREE")
        }
    },
    "ONE"
);

test_deserializer!(
    descriptions_whitespace,
    r#"k-2-3.1_1 The description of enum value deux"#,
    baml_tyannotated!(Category),
    baml_db! {
        enum Category {
            ONE @alias("k1"),
            TWO @alias("k-2-3.1_1"),
            THREE @alias("NUMBER THREE")
        }
    },
    "TWO"
);

test_deserializer!(
    descriptions_period,
    r#"k-2-3.1_1. The description of enum value deux"#,
    baml_tyannotated!(Category),
    baml_db! {
        enum Category {
            ONE @alias("k1"),
            TWO @alias("k-2-3.1_1"),
            THREE @alias("NUMBER THREE")
        }
    },
    "TWO"
);

test_deserializer!(
    alias_with_text,
    r#"I would think k-2-3.1_1 is the best"#,
    baml_tyannotated!(Category),
    baml_db! {
        enum Category {
            ONE @alias("k1"),
            TWO @alias("k-2-3.1_1"),
            THREE @alias("NUMBER THREE")
        }
    },
    "TWO"
);

test_deserializer!(
    multi_aliases,
    r#"k1 is the best! k-2-3.1_1 is bad. k1!"#,
    baml_tyannotated!(Category),
    baml_db! {
        enum Category {
            ONE @alias("k1"),
            TWO @alias("k-2-3.1_1"),
            THREE @alias("NUMBER THREE")
        }
    },
    "ONE"
);

test_deserializer!(
    multi_aliases_1,
    r#"k1 is ok! k-2-3.1_1 is better. I would advise k-2-3.1_1!"#,
    baml_tyannotated!(Category),
    baml_db! {
        enum Category {
            ONE @alias("k1"),
            TWO @alias("k-2-3.1_1"),
            THREE @alias("NUMBER THREE")
        }
    },
    "TWO"
);

// Too many ties
test_failing_deserializer!(
    multi_aliases_2,
    r#"k1 is the best! k-2-3.1_1 is bad. NUMBER_THREE!"#,
    baml_tyannotated!(Category),
    baml_db! {
        enum Category {
            ONE @alias("k1"),
            TWO @alias("k-2-3.1_1"),
            THREE @alias("NUMBER THREE")
        }
    }
);

test_deserializer!(
    list_of_enums,
    r#"["k1", "k-2-3.1_1"]"#,
    baml_tyannotated!([Category]),
    baml_db! {
        enum Category {
            ONE @alias("k1"),
            TWO @alias("k-2-3.1_1"),
            THREE @alias("NUMBER THREE")
        }
    },
    ["ONE", "TWO"]
);

test_deserializer!(
    list_of_enums_2,
    r#"I would think something like this!
```json
[k1, "k-2-3.1_1", "NUMBER THREE"]
```
"#,
    baml_tyannotated!([Category]),
    baml_db! {
        enum Category {
            ONE @alias("k1"),
            TWO @alias("k-2-3.1_1"),
            THREE @alias("NUMBER THREE")
        }
    },
    ["ONE", "TWO", "THREE"]
);

test_deserializer!(
    test_numerical_enum,
    r#"
(such as 1040-X, 1040, etc.) or any payment vouchers.

Based on the criteria provided, this page does not qualify as a tax return form page. Therefore, the appropriate response is:

```json
null
```

This indicates that there is no relevant tax return form type present on the page.
    "#,
    baml_tyannotated!((TaxReturnFormType | null)),
    baml_db! {
        enum TaxReturnFormType {
            F9325 @alias("9325"),
            F9465 @alias("9465"),
            F1040 @alias("1040"),
            F1040X @alias("1040-X")
        }
    },
    null
);

test_failing_deserializer!(
    test_ambiguous_substring_enum,
    "The answer is not car or car-2!",
    baml_tyannotated!(Car),
    baml_db! {
        enum Car {
            A @alias("car"),
            B @alias("car-2")
        }
    }
);

test_deserializer!(
    test_weird_characters,
    r#"
The text "Buy cheap watches now! Limited time offer!!!" is typically characterized by unsolicited
offers and urgency ($^{$_{Ω}$rel}$), which are common traits of spam messages. Therefore, it should be classified as:

- **SPAM**
    "#,
    baml_tyannotated!(MessageType),
    baml_db! {
        enum MessageType {
            SPAM,
            NOT_SPAM
        }
    },
    "SPAM"
);

// test_enum_from_string is skipped because it uses `res.meta_mut().streaming_behavior.done = true`
