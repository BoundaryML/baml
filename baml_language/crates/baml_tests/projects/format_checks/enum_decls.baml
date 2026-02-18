// Testing all the formatter cases for enum declarations

// ===== Minimal enum =====
enum Minimal {
    A
}

// ===== Two variants =====
enum TwoVariants {
    Yes
    No
}

// ===== Multiple variants =====
enum MultiVariant {
    Red
    Green
    Blue
    Yellow
    Purple
}

// ===== Variants with trailing commas =====
enum WithCommas {
    A,
    B,
    C,
}

// ===== Variant with attribute =====
enum WithAttr {
    Active @description("Currently active")
    Inactive @description("Not active")
}

// ===== Variant with multiple attributes =====
enum MultiAttr {
    Success @description("Operation succeeded") @alias("ok")
    Failure @description("Operation failed") @alias("err")
}

// ===== Variant with long attribute exceeding limit =====
enum LongAttr {
    VeryImportantStatus @description("This is an extremely detailed description of the variant that pushes the line well past the default formatter line width of one hundred columns")
}

// ===== Variant with many attributes exceeding limit =====
enum ManyAttrsExceedingLimit {
    ConfiguredVariant @description("A configured variant with many attributes") @alias("configured") @stream.done @stream.not_null @stream.with_state
}

// ===== Long variant name + attributes exceeding limit =====
enum LongVariantPlusAttrs {
    ThisIsAnExtremelyLongVariantNameByItself @description("and it also has a description") @alias("long_variant")
    AnotherVeryLongVariantNameThatNeedsWrapping @description("another long one") @alias("another")
}

// ===== Block attribute on enum =====
enum DynamicEnum {
    X
    Y
    @@dynamic
}

// ===== Trivia on enum declaration (line comments) =====
// 1 enum keyword leading
enum // 2 enum keyword trailing
// 3 name leading
TriviaEnum // 4 name trailing
// 5 open brace leading
{ // 6 open brace trailing
    // 7 first variant leading
    A // 8 first variant trailing
    // 9 second variant leading
    B // 10 second variant trailing
// 11 close brace leading
} // 12 close brace trailing

///////////////////

// ===== Trivia on enum declaration (block comments) =====
/* 1 enum keyword leading */
enum /* 2 enum keyword trailing */
/* 3 name leading */
TriviaEnumBlock /* 4 name trailing */
/* 5 open brace leading */
{ /* 6 open brace trailing */
    /* 7 first variant leading */
    A /* 8 first variant trailing */
    /* 9 second variant leading */
    B /* 10 second variant trailing */
/* 11 close brace leading */
} /* 12 close brace trailing */

///////////////////

// ===== Trivia on variant with attributes (line comments) =====
enum TriviaVariantAttr {
    // 1 variant leading
    Active // 2 variant name trailing
    // 3 attr leading
    @description // 4 attr name trailing
    // 5 open paren leading
    ( // 6 open paren trailing
    // 7 arg leading
    "active" // 8 arg trailing
    // 9 close paren leading
    ) // 10 close paren trailing
}

// ===== Trivia on variant with attributes (block comments) =====
enum TriviaVariantAttrBlock {
    /* 1 variant leading */
    Active /* 2 variant name trailing */
    /* 3 attr leading */
    @description /* 4 attr name trailing */
    /* 5 open paren leading */
    ( /* 6 open paren trailing */
    /* 7 arg leading */
    "active" /* 8 arg trailing */
    /* 9 close paren leading */
    ) /* 10 close paren trailing */
}

// ===== Trivia on wrapping variant with multiple attrs (line comments) =====
enum TriviaWrapVariantAttrs {
    // 1 variant leading
    ConfiguredVariant // 2 variant name trailing
    // 3 first attr leading
    @description // 4 first attr name trailing
    // 5 open paren leading
    ( // 6 open paren trailing
    // 7 arg leading
    "a configured variant with many attributes" // 8 arg trailing
    // 9 close paren leading
    ) // 10 close paren trailing
    // 11 second attr leading
    @alias // 12 second attr name trailing
    // 13 open paren leading
    ( // 14 open paren trailing
    // 15 arg leading
    "configured" // 16 arg trailing
    // 17 close paren leading
    ) // 18 close paren trailing
    // 19 third attr leading
    @stream.done // 20 third attr trailing
    // 21 fourth attr leading
    @stream.not_null // 22 fourth attr trailing
}

// ===== Trivia on wrapping variant with multiple attrs (block comments) =====
enum TriviaWrapVariantAttrsBlock {
    /* 1 variant leading */
    ConfiguredVariant /* 2 variant name trailing */
    /* 3 first attr leading */
    @description /* 4 first attr name trailing */
    /* 5 open paren leading */
    ( /* 6 open paren trailing */
    /* 7 arg leading */
    "a configured variant with many attributes" /* 8 arg trailing */
    /* 9 close paren leading */
    ) /* 10 close paren trailing */
    /* 11 second attr leading */
    @alias /* 12 second attr name trailing */
    /* 13 open paren leading */
    ( /* 14 open paren trailing */
    /* 15 arg leading */
    "configured" /* 16 arg trailing */
    /* 17 close paren leading */
    ) /* 18 close paren trailing */
    /* 19 third attr leading */
    @stream.done /* 20 third attr trailing */
    /* 21 fourth attr leading */
    @stream.not_null /* 22 fourth attr trailing */
}

// ===== Short enum name =====
enum E {
    V
}

// ===== Long enum name exceeding limit =====
enum ThisIsAnExtremelyLongEnumNameThatDefinitelyExceedsTheOneHundredCharacterLineLimitAllByItself {
    Value
}

// ===== Long variant names =====
enum LongVariantNames {
    ThisIsAVeryLongVariantNameThatByItselfExceedsTheOneHundredCharacterLineLimitAllByItselfEasily
    AnotherExtremelyLongVariantNameForTestingLineWrappingBehaviorInTheFormatterOutputDefinitely
    Short
}

// ===== Many variants =====
enum ManyVariants {
    V1
    V2
    V3
    V4
    V5
    V6
    V7
    V8
    V9
    V10
    V11
    V12
}

// ===== Extraneous whitespace =====
enum    ExtraWhitespace    {
    A
    B     @description(  "b"  )     @alias(  "bee"  )
}

// ===== Variant with dotted attribute =====
enum DottedAttrEnum {
    X @stream.done
    Y @stream.not_null
}
