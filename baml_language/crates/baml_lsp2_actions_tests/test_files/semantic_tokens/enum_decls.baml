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
enum BlockAttributedEnum {
    X
    Y
    @@streaming
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

//----
//- diagnostics
// E0098
//
//   × `@@dynamic` was removed; runtime type construction is `baml.reflect` (BEP-066)
//     ╭─[enum_decls.baml:62:5]
//  62 │     @@dynamic
//     ·     ────┬────
//     ·         ╰── removed attribute
//     ╰────
//
//- semantic_tokens
// enum_decls.baml:1:1 (comment) len=56 "// Testing all the formatter cases for enum declarations"
// enum_decls.baml:3:1 (comment) len=27 "// ===== Minimal enum ====="
// enum_decls.baml:4:1 (keyword) len=4 "enum"
// enum_decls.baml:4:6 (enum) [declaration] len=7 "Minimal"
// enum_decls.baml:5:5 (enumMember) [declaration] len=1 "A"
// enum_decls.baml:8:1 (comment) len=27 "// ===== Two variants ====="
// enum_decls.baml:9:1 (keyword) len=4 "enum"
// enum_decls.baml:9:6 (enum) [declaration] len=11 "TwoVariants"
// enum_decls.baml:10:5 (enumMember) [declaration] len=3 "Yes"
// enum_decls.baml:11:5 (enumMember) [declaration] len=2 "No"
// enum_decls.baml:14:1 (comment) len=32 "// ===== Multiple variants ====="
// enum_decls.baml:15:1 (keyword) len=4 "enum"
// enum_decls.baml:15:6 (enum) [declaration] len=12 "MultiVariant"
// enum_decls.baml:16:5 (enumMember) [declaration] len=3 "Red"
// enum_decls.baml:17:5 (enumMember) [declaration] len=5 "Green"
// enum_decls.baml:18:5 (enumMember) [declaration] len=4 "Blue"
// enum_decls.baml:19:5 (enumMember) [declaration] len=6 "Yellow"
// enum_decls.baml:20:5 (enumMember) [declaration] len=6 "Purple"
// enum_decls.baml:23:1 (comment) len=44 "// ===== Variants with trailing commas ====="
// enum_decls.baml:24:1 (keyword) len=4 "enum"
// enum_decls.baml:24:6 (enum) [declaration] len=10 "WithCommas"
// enum_decls.baml:25:5 (enumMember) [declaration] len=1 "A"
// enum_decls.baml:26:5 (enumMember) [declaration] len=1 "B"
// enum_decls.baml:27:5 (enumMember) [declaration] len=1 "C"
// enum_decls.baml:30:1 (comment) len=37 "// ===== Variant with attribute ====="
// enum_decls.baml:31:1 (keyword) len=4 "enum"
// enum_decls.baml:31:6 (enum) [declaration] len=8 "WithAttr"
// enum_decls.baml:32:5 (enumMember) [declaration] len=6 "Active"
// enum_decls.baml:32:12 (decorator) len=1 "@"
// enum_decls.baml:32:13 (decorator) len=11 "description"
// enum_decls.baml:32:25 (string) len=18 "\"Currently active\""
// enum_decls.baml:33:5 (enumMember) [declaration] len=8 "Inactive"
// enum_decls.baml:33:14 (decorator) len=1 "@"
// enum_decls.baml:33:15 (decorator) len=11 "description"
// enum_decls.baml:33:27 (string) len=12 "\"Not active\""
// enum_decls.baml:36:1 (comment) len=47 "// ===== Variant with multiple attributes ====="
// enum_decls.baml:37:1 (keyword) len=4 "enum"
// enum_decls.baml:37:6 (enum) [declaration] len=9 "MultiAttr"
// enum_decls.baml:38:5 (enumMember) [declaration] len=7 "Success"
// enum_decls.baml:38:13 (decorator) len=1 "@"
// enum_decls.baml:38:14 (decorator) len=11 "description"
// enum_decls.baml:38:26 (string) len=21 "\"Operation succeeded\""
// enum_decls.baml:38:49 (decorator) len=1 "@"
// enum_decls.baml:38:50 (decorator) len=5 "alias"
// enum_decls.baml:38:56 (string) len=4 "\"ok\""
// enum_decls.baml:39:5 (enumMember) [declaration] len=7 "Failure"
// enum_decls.baml:39:13 (decorator) len=1 "@"
// enum_decls.baml:39:14 (decorator) len=11 "description"
// enum_decls.baml:39:26 (string) len=18 "\"Operation failed\""
// enum_decls.baml:39:46 (decorator) len=1 "@"
// enum_decls.baml:39:47 (decorator) len=5 "alias"
// enum_decls.baml:39:53 (string) len=5 "\"err\""
// enum_decls.baml:42:1 (comment) len=58 "// ===== Variant with long attribute exceeding limit ====="
// enum_decls.baml:43:1 (keyword) len=4 "enum"
// enum_decls.baml:43:6 (enum) [declaration] len=8 "LongAttr"
// enum_decls.baml:44:5 (enumMember) [declaration] len=19 "VeryImportantStatus"
// enum_decls.baml:44:25 (decorator) len=1 "@"
// enum_decls.baml:44:26 (decorator) len=11 "description"
// enum_decls.baml:44:38 (string) len=145 "\"This is an extremely detailed description of the variant that pushes the line well past the default formatter line width of one hundred columns\""
// enum_decls.baml:47:1 (comment) len=59 "// ===== Variant with many attributes exceeding limit ====="
// enum_decls.baml:48:1 (keyword) len=4 "enum"
// enum_decls.baml:48:6 (enum) [declaration] len=23 "ManyAttrsExceedingLimit"
// enum_decls.baml:49:5 (enumMember) [declaration] len=17 "ConfiguredVariant"
// enum_decls.baml:49:23 (decorator) len=1 "@"
// enum_decls.baml:49:24 (decorator) len=11 "description"
// enum_decls.baml:49:36 (string) len=43 "\"A configured variant with many attributes\""
// enum_decls.baml:49:81 (decorator) len=1 "@"
// enum_decls.baml:49:82 (decorator) len=5 "alias"
// enum_decls.baml:49:88 (string) len=12 "\"configured\""
// enum_decls.baml:49:102 (decorator) len=1 "@"
// enum_decls.baml:49:103 (decorator) len=6 "stream"
// enum_decls.baml:49:110 (decorator) len=4 "done"
// enum_decls.baml:49:115 (decorator) len=1 "@"
// enum_decls.baml:49:116 (decorator) len=6 "stream"
// enum_decls.baml:49:123 (decorator) len=8 "not_null"
// enum_decls.baml:49:132 (decorator) len=1 "@"
// enum_decls.baml:49:133 (decorator) len=6 "stream"
// enum_decls.baml:49:140 (decorator) len=10 "with_state"
// enum_decls.baml:52:1 (comment) len=61 "// ===== Long variant name + attributes exceeding limit ====="
// enum_decls.baml:53:1 (keyword) len=4 "enum"
// enum_decls.baml:53:6 (enum) [declaration] len=20 "LongVariantPlusAttrs"
// enum_decls.baml:54:5 (enumMember) [declaration] len=40 "ThisIsAnExtremelyLongVariantNameByItself"
// enum_decls.baml:54:46 (decorator) len=1 "@"
// enum_decls.baml:54:47 (decorator) len=11 "description"
// enum_decls.baml:54:59 (string) len=31 "\"and it also has a description\""
// enum_decls.baml:54:92 (decorator) len=1 "@"
// enum_decls.baml:54:93 (decorator) len=5 "alias"
// enum_decls.baml:54:99 (string) len=14 "\"long_variant\""
// enum_decls.baml:55:5 (enumMember) [declaration] len=43 "AnotherVeryLongVariantNameThatNeedsWrapping"
// enum_decls.baml:55:49 (decorator) len=1 "@"
// enum_decls.baml:55:50 (decorator) len=11 "description"
// enum_decls.baml:55:62 (string) len=18 "\"another long one\""
// enum_decls.baml:55:82 (decorator) len=1 "@"
// enum_decls.baml:55:83 (decorator) len=5 "alias"
// enum_decls.baml:55:89 (string) len=9 "\"another\""
// enum_decls.baml:58:1 (comment) len=38 "// ===== Block attribute on enum ====="
// enum_decls.baml:59:1 (keyword) len=4 "enum"
// enum_decls.baml:59:6 (enum) [declaration] len=19 "BlockAttributedEnum"
// enum_decls.baml:60:5 (enumMember) [declaration] len=1 "X"
// enum_decls.baml:61:5 (enumMember) [declaration] len=1 "Y"
// enum_decls.baml:62:5 (decorator) len=2 "@@"
// enum_decls.baml:62:7 (decorator) len=9 "streaming"
// enum_decls.baml:65:1 (comment) len=57 "// ===== Trivia on enum declaration (line comments) ====="
// enum_decls.baml:66:1 (comment) len=25 "// 1 enum keyword leading"
// enum_decls.baml:67:1 (keyword) len=4 "enum"
// enum_decls.baml:67:6 (comment) len=26 "// 2 enum keyword trailing"
// enum_decls.baml:68:1 (comment) len=17 "// 3 name leading"
// enum_decls.baml:69:1 (enum) [declaration] len=10 "TriviaEnum"
// enum_decls.baml:69:12 (comment) len=18 "// 4 name trailing"
// enum_decls.baml:70:1 (comment) len=23 "// 5 open brace leading"
// enum_decls.baml:71:3 (comment) len=24 "// 6 open brace trailing"
// enum_decls.baml:72:5 (comment) len=26 "// 7 first variant leading"
// enum_decls.baml:73:5 (enumMember) [declaration] len=1 "A"
// enum_decls.baml:73:7 (comment) len=27 "// 8 first variant trailing"
// enum_decls.baml:74:5 (comment) len=27 "// 9 second variant leading"
// enum_decls.baml:75:5 (enumMember) [declaration] len=1 "B"
// enum_decls.baml:75:7 (comment) len=29 "// 10 second variant trailing"
// enum_decls.baml:76:1 (comment) len=25 "// 11 close brace leading"
// enum_decls.baml:77:3 (comment) len=26 "// 12 close brace trailing"
// enum_decls.baml:79:1 (comment) len=19 "///////////////////"
// enum_decls.baml:81:1 (comment) len=58 "// ===== Trivia on enum declaration (block comments) ====="
// enum_decls.baml:82:1 (comment) len=28 "/* 1 enum keyword leading */"
// enum_decls.baml:83:1 (keyword) len=4 "enum"
// enum_decls.baml:83:6 (comment) len=29 "/* 2 enum keyword trailing */"
// enum_decls.baml:84:1 (comment) len=20 "/* 3 name leading */"
// enum_decls.baml:85:1 (enum) [declaration] len=15 "TriviaEnumBlock"
// enum_decls.baml:85:17 (comment) len=21 "/* 4 name trailing */"
// enum_decls.baml:86:1 (comment) len=26 "/* 5 open brace leading */"
// enum_decls.baml:87:3 (comment) len=27 "/* 6 open brace trailing */"
// enum_decls.baml:88:5 (comment) len=29 "/* 7 first variant leading */"
// enum_decls.baml:89:5 (enumMember) [declaration] len=1 "A"
// enum_decls.baml:89:7 (comment) len=30 "/* 8 first variant trailing */"
// enum_decls.baml:90:5 (comment) len=30 "/* 9 second variant leading */"
// enum_decls.baml:91:5 (enumMember) [declaration] len=1 "B"
// enum_decls.baml:91:7 (comment) len=32 "/* 10 second variant trailing */"
// enum_decls.baml:92:1 (comment) len=28 "/* 11 close brace leading */"
// enum_decls.baml:93:3 (comment) len=29 "/* 12 close brace trailing */"
// enum_decls.baml:95:1 (comment) len=19 "///////////////////"
// enum_decls.baml:97:1 (comment) len=64 "// ===== Trivia on variant with attributes (line comments) ====="
// enum_decls.baml:98:1 (keyword) len=4 "enum"
// enum_decls.baml:98:6 (enum) [declaration] len=17 "TriviaVariantAttr"
// enum_decls.baml:99:5 (comment) len=20 "// 1 variant leading"
// enum_decls.baml:100:5 (enumMember) [declaration] len=6 "Active"
// enum_decls.baml:100:12 (comment) len=26 "// 2 variant name trailing"
// enum_decls.baml:101:5 (comment) len=17 "// 3 attr leading"
// enum_decls.baml:102:5 (decorator) len=1 "@"
// enum_decls.baml:102:6 (decorator) len=11 "description"
// enum_decls.baml:102:18 (comment) len=23 "// 4 attr name trailing"
// enum_decls.baml:103:5 (comment) len=23 "// 5 open paren leading"
// enum_decls.baml:104:7 (comment) len=24 "// 6 open paren trailing"
// enum_decls.baml:105:5 (comment) len=16 "// 7 arg leading"
// enum_decls.baml:106:5 (string) len=8 "\"active\""
// enum_decls.baml:106:14 (comment) len=17 "// 8 arg trailing"
// enum_decls.baml:107:5 (comment) len=24 "// 9 close paren leading"
// enum_decls.baml:108:7 (comment) len=26 "// 10 close paren trailing"
// enum_decls.baml:111:1 (comment) len=65 "// ===== Trivia on variant with attributes (block comments) ====="
// enum_decls.baml:112:1 (keyword) len=4 "enum"
// enum_decls.baml:112:6 (enum) [declaration] len=22 "TriviaVariantAttrBlock"
// enum_decls.baml:113:5 (comment) len=23 "/* 1 variant leading */"
// enum_decls.baml:114:5 (enumMember) [declaration] len=6 "Active"
// enum_decls.baml:114:12 (comment) len=29 "/* 2 variant name trailing */"
// enum_decls.baml:115:5 (comment) len=20 "/* 3 attr leading */"
// enum_decls.baml:116:5 (decorator) len=1 "@"
// enum_decls.baml:116:6 (decorator) len=11 "description"
// enum_decls.baml:116:18 (comment) len=26 "/* 4 attr name trailing */"
// enum_decls.baml:117:5 (comment) len=26 "/* 5 open paren leading */"
// enum_decls.baml:118:7 (comment) len=27 "/* 6 open paren trailing */"
// enum_decls.baml:119:5 (comment) len=19 "/* 7 arg leading */"
// enum_decls.baml:120:5 (string) len=8 "\"active\""
// enum_decls.baml:120:14 (comment) len=20 "/* 8 arg trailing */"
// enum_decls.baml:121:5 (comment) len=27 "/* 9 close paren leading */"
// enum_decls.baml:122:7 (comment) len=29 "/* 10 close paren trailing */"
// enum_decls.baml:125:1 (comment) len=77 "// ===== Trivia on wrapping variant with multiple attrs (line comments) ====="
// enum_decls.baml:126:1 (keyword) len=4 "enum"
// enum_decls.baml:126:6 (enum) [declaration] len=22 "TriviaWrapVariantAttrs"
// enum_decls.baml:127:5 (comment) len=20 "// 1 variant leading"
// enum_decls.baml:128:5 (enumMember) [declaration] len=17 "ConfiguredVariant"
// enum_decls.baml:128:23 (comment) len=26 "// 2 variant name trailing"
// enum_decls.baml:129:5 (comment) len=23 "// 3 first attr leading"
// enum_decls.baml:130:5 (decorator) len=1 "@"
// enum_decls.baml:130:6 (decorator) len=11 "description"
// enum_decls.baml:130:18 (comment) len=29 "// 4 first attr name trailing"
// enum_decls.baml:131:5 (comment) len=23 "// 5 open paren leading"
// enum_decls.baml:132:7 (comment) len=24 "// 6 open paren trailing"
// enum_decls.baml:133:5 (comment) len=16 "// 7 arg leading"
// enum_decls.baml:134:5 (string) len=43 "\"a configured variant with many attributes\""
// enum_decls.baml:134:49 (comment) len=17 "// 8 arg trailing"
// enum_decls.baml:135:5 (comment) len=24 "// 9 close paren leading"
// enum_decls.baml:136:7 (comment) len=26 "// 10 close paren trailing"
// enum_decls.baml:137:5 (comment) len=25 "// 11 second attr leading"
// enum_decls.baml:138:5 (decorator) len=1 "@"
// enum_decls.baml:138:6 (decorator) len=5 "alias"
// enum_decls.baml:138:12 (comment) len=31 "// 12 second attr name trailing"
// enum_decls.baml:139:5 (comment) len=24 "// 13 open paren leading"
// enum_decls.baml:140:7 (comment) len=25 "// 14 open paren trailing"
// enum_decls.baml:141:5 (comment) len=17 "// 15 arg leading"
// enum_decls.baml:142:5 (string) len=12 "\"configured\""
// enum_decls.baml:142:18 (comment) len=18 "// 16 arg trailing"
// enum_decls.baml:143:5 (comment) len=25 "// 17 close paren leading"
// enum_decls.baml:144:7 (comment) len=26 "// 18 close paren trailing"
// enum_decls.baml:145:5 (comment) len=24 "// 19 third attr leading"
// enum_decls.baml:146:5 (decorator) len=1 "@"
// enum_decls.baml:146:6 (decorator) len=6 "stream"
// enum_decls.baml:146:13 (decorator) len=4 "done"
// enum_decls.baml:146:18 (comment) len=25 "// 20 third attr trailing"
// enum_decls.baml:147:5 (comment) len=25 "// 21 fourth attr leading"
// enum_decls.baml:148:5 (decorator) len=1 "@"
// enum_decls.baml:148:6 (decorator) len=6 "stream"
// enum_decls.baml:148:13 (decorator) len=8 "not_null"
// enum_decls.baml:148:22 (comment) len=26 "// 22 fourth attr trailing"
// enum_decls.baml:151:1 (comment) len=78 "// ===== Trivia on wrapping variant with multiple attrs (block comments) ====="
// enum_decls.baml:152:1 (keyword) len=4 "enum"
// enum_decls.baml:152:6 (enum) [declaration] len=27 "TriviaWrapVariantAttrsBlock"
// enum_decls.baml:153:5 (comment) len=23 "/* 1 variant leading */"
// enum_decls.baml:154:5 (enumMember) [declaration] len=17 "ConfiguredVariant"
// enum_decls.baml:154:23 (comment) len=29 "/* 2 variant name trailing */"
// enum_decls.baml:155:5 (comment) len=26 "/* 3 first attr leading */"
// enum_decls.baml:156:5 (decorator) len=1 "@"
// enum_decls.baml:156:6 (decorator) len=11 "description"
// enum_decls.baml:156:18 (comment) len=32 "/* 4 first attr name trailing */"
// enum_decls.baml:157:5 (comment) len=26 "/* 5 open paren leading */"
// enum_decls.baml:158:7 (comment) len=27 "/* 6 open paren trailing */"
// enum_decls.baml:159:5 (comment) len=19 "/* 7 arg leading */"
// enum_decls.baml:160:5 (string) len=43 "\"a configured variant with many attributes\""
// enum_decls.baml:160:49 (comment) len=20 "/* 8 arg trailing */"
// enum_decls.baml:161:5 (comment) len=27 "/* 9 close paren leading */"
// enum_decls.baml:162:7 (comment) len=29 "/* 10 close paren trailing */"
// enum_decls.baml:163:5 (comment) len=28 "/* 11 second attr leading */"
// enum_decls.baml:164:5 (decorator) len=1 "@"
// enum_decls.baml:164:6 (decorator) len=5 "alias"
// enum_decls.baml:164:12 (comment) len=34 "/* 12 second attr name trailing */"
// enum_decls.baml:165:5 (comment) len=27 "/* 13 open paren leading */"
// enum_decls.baml:166:7 (comment) len=28 "/* 14 open paren trailing */"
// enum_decls.baml:167:5 (comment) len=20 "/* 15 arg leading */"
// enum_decls.baml:168:5 (string) len=12 "\"configured\""
// enum_decls.baml:168:18 (comment) len=21 "/* 16 arg trailing */"
// enum_decls.baml:169:5 (comment) len=28 "/* 17 close paren leading */"
// enum_decls.baml:170:7 (comment) len=29 "/* 18 close paren trailing */"
// enum_decls.baml:171:5 (comment) len=27 "/* 19 third attr leading */"
// enum_decls.baml:172:5 (decorator) len=1 "@"
// enum_decls.baml:172:6 (decorator) len=6 "stream"
// enum_decls.baml:172:13 (decorator) len=4 "done"
// enum_decls.baml:172:18 (comment) len=28 "/* 20 third attr trailing */"
// enum_decls.baml:173:5 (comment) len=28 "/* 21 fourth attr leading */"
// enum_decls.baml:174:5 (decorator) len=1 "@"
// enum_decls.baml:174:6 (decorator) len=6 "stream"
// enum_decls.baml:174:13 (decorator) len=8 "not_null"
// enum_decls.baml:174:22 (comment) len=29 "/* 22 fourth attr trailing */"
// enum_decls.baml:177:1 (comment) len=30 "// ===== Short enum name ====="
// enum_decls.baml:178:1 (keyword) len=4 "enum"
// enum_decls.baml:178:6 (enum) [declaration] len=1 "E"
// enum_decls.baml:179:5 (enumMember) [declaration] len=1 "V"
// enum_decls.baml:182:1 (comment) len=45 "// ===== Long enum name exceeding limit ====="
// enum_decls.baml:183:1 (keyword) len=4 "enum"
// enum_decls.baml:183:6 (enum) [declaration] len=92 "ThisIsAnExtremelyLongEnumNameThatDefinitelyExceedsTheOneHundredCharacterLineLimitAllByItself"
// enum_decls.baml:184:5 (enumMember) [declaration] len=5 "Value"
// enum_decls.baml:187:1 (comment) len=33 "// ===== Long variant names ====="
// enum_decls.baml:188:1 (keyword) len=4 "enum"
// enum_decls.baml:188:6 (enum) [declaration] len=16 "LongVariantNames"
// enum_decls.baml:189:5 (enumMember) [declaration] len=93 "ThisIsAVeryLongVariantNameThatByItselfExceedsTheOneHundredCharacterLineLimitAllByItselfEasily"
// enum_decls.baml:190:5 (enumMember) [declaration] len=91 "AnotherExtremelyLongVariantNameForTestingLineWrappingBehaviorInTheFormatterOutputDefinitely"
// enum_decls.baml:191:5 (enumMember) [declaration] len=5 "Short"
// enum_decls.baml:194:1 (comment) len=28 "// ===== Many variants ====="
// enum_decls.baml:195:1 (keyword) len=4 "enum"
// enum_decls.baml:195:6 (enum) [declaration] len=12 "ManyVariants"
// enum_decls.baml:196:5 (enumMember) [declaration] len=2 "V1"
// enum_decls.baml:197:5 (enumMember) [declaration] len=2 "V2"
// enum_decls.baml:198:5 (enumMember) [declaration] len=2 "V3"
// enum_decls.baml:199:5 (enumMember) [declaration] len=2 "V4"
// enum_decls.baml:200:5 (enumMember) [declaration] len=2 "V5"
// enum_decls.baml:201:5 (enumMember) [declaration] len=2 "V6"
// enum_decls.baml:202:5 (enumMember) [declaration] len=2 "V7"
// enum_decls.baml:203:5 (enumMember) [declaration] len=2 "V8"
// enum_decls.baml:204:5 (enumMember) [declaration] len=2 "V9"
// enum_decls.baml:205:5 (enumMember) [declaration] len=3 "V10"
// enum_decls.baml:206:5 (enumMember) [declaration] len=3 "V11"
// enum_decls.baml:207:5 (enumMember) [declaration] len=3 "V12"
// enum_decls.baml:210:1 (comment) len=36 "// ===== Extraneous whitespace ====="
// enum_decls.baml:211:1 (keyword) len=4 "enum"
// enum_decls.baml:211:9 (enum) [declaration] len=15 "ExtraWhitespace"
// enum_decls.baml:212:5 (enumMember) [declaration] len=1 "A"
// enum_decls.baml:213:5 (enumMember) [declaration] len=1 "B"
// enum_decls.baml:213:11 (decorator) len=1 "@"
// enum_decls.baml:213:12 (decorator) len=11 "description"
// enum_decls.baml:213:26 (string) len=3 "\"b\""
// enum_decls.baml:213:37 (decorator) len=1 "@"
// enum_decls.baml:213:38 (decorator) len=5 "alias"
// enum_decls.baml:213:46 (string) len=5 "\"bee\""
// enum_decls.baml:216:1 (comment) len=44 "// ===== Variant with dotted attribute ====="
// enum_decls.baml:217:1 (keyword) len=4 "enum"
// enum_decls.baml:217:6 (enum) [declaration] len=14 "DottedAttrEnum"
// enum_decls.baml:218:5 (enumMember) [declaration] len=1 "X"
// enum_decls.baml:218:7 (decorator) len=1 "@"
// enum_decls.baml:218:8 (decorator) len=6 "stream"
// enum_decls.baml:218:15 (decorator) len=4 "done"
// enum_decls.baml:219:5 (enumMember) [declaration] len=1 "Y"
// enum_decls.baml:219:7 (decorator) len=1 "@"
// enum_decls.baml:219:8 (decorator) len=6 "stream"
// enum_decls.baml:219:15 (decorator) len=8 "not_null"
