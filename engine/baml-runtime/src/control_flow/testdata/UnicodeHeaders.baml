// Edge cases for MDX headers

function UnicodeHeaders() -> string {
    //# 你好世界
    // Unicode header test

    //## Émojis 🎉🎊
    // Headers with emojis

    //### Special «Characters» ™

    "test"
}

function HeaderLevels() -> string {
    //# Level 1
    //## Level 2
    //### Level 3
    //#### Level 4
    //##### Level 5
    //###### Level 6
    //####### Level 7 - More than 6 levels
    //######## Level 8
    //######### Level 9

    "deep nesting"
}

function HeadersWithWhitespace() -> string {
    //#    Extra Spaces Before Title
    //##  	Mixed Tabs And Spaces
    //###			All Tabs

    let x = "whitespace test";

    //# Trailing Spaces
    //## Multiple  Spaces  Between  Words

    x
}

function HeadersAndComments() -> string {
    //# Header Before Comment
    // This is a regular comment

    let x = "test";

    // Comment before header
    //## Header After Comment

    /// Doc comment before header
    //### Header After Doc Comment

    x
}

function HeadersInComplexNesting() -> string {
    //# Main Function

    let result = {
        //## Inside Block Expression

        let inner = if (true) {
            //### Inside If True

            let deep = {
                //#### Very Deep Block
                "deep value"
            };

            //### Back to If Level
            deep
        } else {
            //### Inside Else
            "else value"
        };

        //## Back to Block Level
        inner
    };

    //# Back to Function Level
    result
}

function HeadersWithSpecialChars() -> string {
    //# Header-With-Dashes
    //## Header_With_Underscores
    //### Header.With.Dots
    //#### Header/With/Slashes
    //##### Header\With\Backslashes
    //###### Header|With|Pipes

    "special chars"
}

function ConsecutiveHeaders() -> string {
    //# First Header
    //# Second Header
    //# Third Header

    //## Subsection A
    //## Subsection B
    //## Subsection C

    "multiple consecutive headers"
}

function HeadersAroundExpressions() -> int {
    //# Before Let
    let x = 5;
    //## After Let

    //### Before If
    if (x > 0) {
        //#### Inside If
        x
    } else {
        //#### Inside Else
        0
    }
    //### After If

    //## Before For
    for (let i in [1, 2, 3]) {
        //### Inside For
        i
    }
    //## After For

    //# Final Section
    x
}

function VeryLongHeaderTitle() -> string {
    //# This Is An Extremely Long Header Title That Goes On And On And On And Contains Many Words To Test How The Parser Handles Very Long Header Titles That Might Span Multiple Lines In Some Editors But Should Still Be Parsed As A Single Header

    //## Another Long Title With Special Characters !@#$%^&*()_+-=[]{}|;':",./<>? And Numbers 1234567890

    "long headers"
}

function HeaderOnlyFunction() -> string {
    //# Just Headers
    //## No Code
    //### Only Documentation
    //#### Pure Annotation
    //##### No Expressions
    //###### Until The End

    "default"
}

// warning: Workflow functions are experimental, and will break in the future.
//   -->  headers/mdx_headers_edge_cases.baml:3
//    | 
//  2 | 
//  3 | function UnicodeHeaders() -> string {
//    | 
// warning: Workflow functions are experimental, and will break in the future.
//   -->  headers/mdx_headers_edge_cases.baml:15
//    | 
// 14 | 
// 15 | function HeaderLevels() -> string {
//    | 
// warning: Workflow functions are experimental, and will break in the future.
//   -->  headers/mdx_headers_edge_cases.baml:29
//    | 
// 28 | 
// 29 | function HeadersWithWhitespace() -> string {
//    | 
// warning: Workflow functions are experimental, and will break in the future.
//   -->  headers/mdx_headers_edge_cases.baml:42
//    | 
// 41 | 
// 42 | function HeadersAndComments() -> string {
//    | 
// warning: Workflow functions are experimental, and will break in the future.
//   -->  headers/mdx_headers_edge_cases.baml:57
//    | 
// 56 | 
// 57 | function HeadersInComplexNesting() -> string {
//    | 
// warning: Workflow functions are experimental, and will break in the future.
//   -->  headers/mdx_headers_edge_cases.baml:86
//    | 
// 85 | 
// 86 | function HeadersWithSpecialChars() -> string {
//    | 
// warning: Workflow functions are experimental, and will break in the future.
//   -->  headers/mdx_headers_edge_cases.baml:97
//    | 
// 96 | 
// 97 | function ConsecutiveHeaders() -> string {
//    | 
// warning: Workflow functions are experimental, and will break in the future.
//   -->  headers/mdx_headers_edge_cases.baml:109
//    | 
// 108 | 
// 109 | function HeadersAroundExpressions() -> string {
//    | 
// warning: Workflow functions are experimental, and will break in the future.
//   -->  headers/mdx_headers_edge_cases.baml:143
//    | 
// 142 | 
// 143 | function HeaderOnlyFunction() -> string {
//    | 
// error: Unknown variable deep
//   -->  headers/mdx_headers_edge_cases.baml:72
//    | 
// 71 |             ### Back to If Level
// 72 |             deep
//    | 
