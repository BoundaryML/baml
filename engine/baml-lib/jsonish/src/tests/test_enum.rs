use baml_types::type_meta::base::TypeMeta;

use super::*;

const ENUM_FILE: &str = r#"
// Enums
enum Category {
ONE
TWO
}
"#;

const PASCAL_CASE_ENUM_FILE: &str = r#"
// Enums
enum PascalCaseCategory {
One
Two
}
"#;

test_deserializer!(
    test_enum,
    ENUM_FILE,
    r#"TWO"#,
    TypeIR::r#enum("Category"),
    "TWO"
);

test_deserializer!(
    case_insensitive,
    ENUM_FILE,
    r#"two"#,
    TypeIR::r#enum("Category"),
    "TWO"
);

test_deserializer!(
    with_quotes,
    ENUM_FILE,
    r#""TWO""#,
    TypeIR::r#enum("Category"),
    "TWO"
);

test_deserializer!(
    from_enum_list_single,
    ENUM_FILE,
    r#"["TWO"]"#,
    TypeIR::r#enum("Category"),
    "TWO"
);

test_deserializer!(
    from_enum_list_multi,
    ENUM_FILE,
    r#"["TWO", "THREE"]"#,
    TypeIR::r#enum("Category"),
    "TWO"
);

test_deserializer!(
    from_string_with_extra_text_after_1,
    ENUM_FILE,
    r#""ONE: The description of k1""#,
    TypeIR::r#enum("Category"),
    "ONE"
);

test_deserializer!(
    from_string_and_case_mismatch,
    ENUM_FILE,
    "The answer is One",
    TypeIR::r#enum("Category"),
    "ONE"
);

test_deserializer!(
    from_string_and_case_mismatch_wrapped,
    ENUM_FILE,
    "**one** is the answer",
    TypeIR::r#enum("Category"),
    "ONE"
);

test_deserializer!(
    from_string_and_case_mismatch_upper,
    PASCAL_CASE_ENUM_FILE,
    "**ONE** is the answer",
    TypeIR::r#enum("PascalCaseCategory"),
    "One"
);

test_deserializer!(
    from_string_with_extra_text_after_2,
    ENUM_FILE,
    r#""ONE - The description of an enum value""#,
    TypeIR::r#enum("Category"),
    "ONE"
);

test_deserializer!(
    case_sensitive_non_ambiguous_match,
    ENUM_FILE,
    r#"TWO" is one of the correct answers."#,
    TypeIR::r#enum("Category"),
    "TWO"
);

test_failing_deserializer!(
    case_insensitive_ambiguous_match,
    ENUM_FILE,
    r#"Two" is one of the correct answers."#,
    TypeIR::r#enum("Category")
);

test_failing_deserializer!(
    from_string_with_extra_text_after_3,
    ENUM_FILE,
    r#""ONE - is the answer, not TWO""#,
    TypeIR::r#enum("Category")
);

test_failing_deserializer!(
    from_string_with_extra_text_after_4,
    ENUM_FILE,
    r#""ONE. is the answer, not TWO""#,
    TypeIR::r#enum("Category")
);

test_failing_deserializer!(
    from_string_with_extra_text_after_5,
    ENUM_FILE,
    r#""ONE: is the answer, not TWO""#,
    TypeIR::r#enum("Category")
);

const ENUM_FILE_WITH_DESCRIPTIONS: &str = r#"
// Enums
enum Category {
ONE @alias(k1) @description("The description of enum value une")
TWO @alias("k-2-3.1_1") @description("The description of enum value deux")
THREE @alias(NUMBER THREE)
}
"#;

test_deserializer!(
    aliases_1,
    ENUM_FILE_WITH_DESCRIPTIONS,
    r#"k1"#,
    TypeIR::r#enum("Category"),
    "ONE"
);

test_deserializer!(
    aliases_2,
    ENUM_FILE_WITH_DESCRIPTIONS,
    r#"k-2-3.1_1"#,
    TypeIR::r#enum("Category"),
    "TWO"
);

test_deserializer!(
    aliases_3,
    ENUM_FILE_WITH_DESCRIPTIONS,
    r#"NUMBER THREE"#,
    TypeIR::r#enum("Category"),
    "THREE"
);

test_deserializer!(
    no_punctuation,
    ENUM_FILE_WITH_DESCRIPTIONS,
    r#"number three"#,
    TypeIR::r#enum("Category"),
    "THREE"
);

test_deserializer!(
    no_punctuation_2,
    ENUM_FILE_WITH_DESCRIPTIONS,
    r#"k-2-3 1_1"#,
    TypeIR::r#enum("Category"),
    "TWO"
);

test_deserializer!(
    descriptions,
    ENUM_FILE_WITH_DESCRIPTIONS,
    r#"k1: The description of enum value une"#,
    TypeIR::r#enum("Category"),
    "ONE"
);

test_deserializer!(
    descriptions_whitespace,
    ENUM_FILE_WITH_DESCRIPTIONS,
    r#"k-2-3.1_1 The description of enum value deux"#,
    TypeIR::r#enum("Category"),
    "TWO"
);

test_deserializer!(
    descriptions_period,
    ENUM_FILE_WITH_DESCRIPTIONS,
    r#"k-2-3.1_1. The description of enum value deux"#,
    TypeIR::r#enum("Category"),
    "TWO"
);

test_deserializer!(
    alias_with_text,
    ENUM_FILE_WITH_DESCRIPTIONS,
    r#"I would think k-2-3.1_1 is the best"#,
    TypeIR::r#enum("Category"),
    "TWO"
);

test_deserializer!(
    multi_aliases,
    ENUM_FILE_WITH_DESCRIPTIONS,
    r#"k1 is the best! k-2-3.1_1 is bad. k1!"#,
    TypeIR::r#enum("Category"),
    "ONE"
);

test_deserializer!(
    multi_aliases_1,
    ENUM_FILE_WITH_DESCRIPTIONS,
    r#"k1 is ok! k-2-3.1_1 is better. I would advise k-2-3.1_1!"#,
    TypeIR::r#enum("Category"),
    "TWO"
);

// Too many ties
test_failing_deserializer!(
    multi_aliases_2,
    ENUM_FILE_WITH_DESCRIPTIONS,
    r#"k1 is the best! k-2-3.1_1 is bad. NUMBER_THREE!"#,
    TypeIR::r#enum("Category")
);

test_deserializer!(
    list_of_enums,
    ENUM_FILE_WITH_DESCRIPTIONS,
    r#"["k1", "k-2-3.1_1"]"#,
    TypeIR::list(TypeIR::r#enum("Category")),
    ["ONE", "TWO"]
);

test_deserializer!(
    list_of_enums_2,
    ENUM_FILE_WITH_DESCRIPTIONS,
    r#"I would think something like this!
```json    
[k1, "k-2-3.1_1", "NUMBER THREE"]
```
"#,
    TypeIR::list(TypeIR::r#enum("Category")),
    ["ONE", "TWO", "THREE"]
);

test_deserializer!(
    test_numerical_enum,
    r#"
enum TaxReturnFormType {
    F9325 @alias("9325")
    F9465 @alias("9465")
    F1040 @alias("1040")
    F1040X @alias("1040-X")
}
"#,
    r#"
(such as 1040-X, 1040, etc.) or any payment vouchers.

Based on the criteria provided, this page does not qualify as a tax return form page. Therefore, the appropriate response is:

```json
null
``` 

This indicates that there is no relevant tax return form type present on the page.
    "#,
    TypeIR::r#enum("TaxReturnFormType").as_optional(),
    null
);

test_failing_deserializer!(
    test_ambiguous_substring_enum,
    r#"
        enum Car {
            A @alias("car")
            B @alias("car-2")
        }
    "#,
    "The answer is not car or car-2!",
    TypeIR::r#enum("Car")
);

test_deserializer!(
    test_weird_characters,
    r#"
enum MessageType {
  SPAM
  NOT_SPAM
}
    "#,
    r#"
The text "Buy cheap watches now! Limited time offer!!!" is typically characterized by unsolicited 
offers and urgency ($^{$_{Ω}$rel}$), which are common traits of spam messages. Therefore, it should be classified as:

- **SPAM**
    "#,
    TypeIR::r#enum("MessageType"),
    "SPAM"
);

test_deserializer!(
    test_enum_from_string,
    r#"
enum MessageType {
  SPAM @alias("k5")
  NOT_SPAM @alias("k6")
}
    "#,
    " `k5`\n\nThe category \"k5: User is excited\" is designed to identify and classify user inputs that express strong positive emotions, enthusiasm, or anticipation. This classification applies when the language used by the user conveys an eagerness or thrill about something they are experiencing or expecting.\n\n### Characteristics of Excitement\n- **Emotional Expressions:** The use of exclamation marks, emphatic words like \"amazing,\" \"incredible,\" or \"fantastic.\"\n- **Positive Language:** Use of positive adjectives and adverbs such as \"can't wait,\" \"thrilled,\" \"excited,\" or \"elated.\"\n- **Anticipation:** Statements that show looking forward to an event, result, or item.\n  \n### Examples\n- *\"I can’t wait for the concert tonight! It's going to be amazing!\"*\n- *\"This new game release has me super excited. I've been waiting months for this!\"*\n\n### Long Description:\nWhen a user demonstrates excitement in their communication, it generally reflects an emotional high, eagerness, or intense positivity regarding whatever they are discussing. This could pertain to events like attending a sports game or concert, receiving positive news or achievements, encountering something novel and stimulating (like a new gadget or experience), or anticipating something eagerly awaited.\n\nThe user’s input might include dynamic language that conveys an elevated state of anticipation or satisfaction with an imminent or forthcoming occurrence. Often associated with increased energy levels in the text itself—through phrases like \"so excited!\" or actions (\"counting down until\") — this category taps into the positive psychology aspects, depicting a scenario where the user feels joyous eagerness and anticipatory pleasure.\n\nUnderstanding excitement is crucial because it can drive engagement, motivation, and personal enthusiasm which might influence decision-making and behavior. Recognizing exciting expressions helps in tailoring responses or actions that resonate with the user's emotional state, maintaining an enthusiastic interaction, and potentially amplifying positive outcomes.",
    {
        let mut res = TypeIR::r#enum("MessageType");
        res.meta_mut().streaming_behavior.done = true;
        res
    },
    "SPAM"
);
