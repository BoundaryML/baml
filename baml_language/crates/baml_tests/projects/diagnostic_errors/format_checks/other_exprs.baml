// Testing all the formatter cases for expressions that don't have their own files

class OtherPoint {
    x int
    y int
}

class OtherVec {
    p OtherPoint
    q OtherPoint
}

class OtherTriple {
    a OtherVec
    b OtherVec
    c OtherVec
}

function OtherExprs(a: int, b: int) -> int {

    // ===== Integer literals =====
    0;
    1;
    42;
    999999;

    // ===== Float literals =====
    0.0;
    3.14;
    99.999;

    // ===== String literals =====
    "hello";
    "a short string";
    "a much longer string that contains quite a lot of text and definitely exceeds one hundred characters to test how the formatter handles longer string literals in expressions";

    // ===== Boolean literals =====
    true;
    false;

    // ===== Null literal =====
    null;

    // ===== Raw string literals =====
    #"raw string"#;
    #"raw string with "quotes" inside"#;
    #"
        multi-line raw string
        with several lines
        of content
    "#;

    // ===== Parenthesized expressions =====
    (a);
    (a + b);
    ((a));
    ((a + b) * (a - b));

    // ===== Deeply nested parenthesized exceeding limit =====
    ((((a + b) * (a - b)) + ((a * a) + (b * b))) * (((a + b) * (a + b)) - ((a - b) * (a - b))));

    // ===== Trivia in parens (line comments) =====
    // 1 open leading
    ( // 2 open trailing
    // 3 inner leading
    a // 4 inner trailing
    // 5 close leading
    ) // 6 close trailing
    ;

    // ===== Trivia in parens (block comments) =====
    /* 1 open leading */
    ( /* 2 open trailing */
    /* 3 inner leading */
    a /* 4 inner trailing */
    /* 5 close leading */
    ) /* 6 close trailing */
    ;

    // ===== Unary expressions =====
    -a;
    !true;
    -(-a);
    !(a > b);

    // ===== Trivia in unary (line comments) =====
    // 1 op leading
    - // 2 op trailing
    // 3 operand leading
    a // 4 operand trailing
    ;

    // ===== Trivia in unary (block comments) =====
    /* 1 op leading */
    - /* 2 op trailing */
    /* 3 operand leading */
    a /* 4 operand trailing */
    ;

    // ===== Array initializers =====
    [];
    [1];
    [1, 2, 3];
    [1, 2, 3, 4, 5];
    ["short", "array"];
    ["a", "b", "c", "d", "e", "f", "g", "h", "i", "j", "k", "l", "m", "n", "o", "p", "q"];

    // ===== Long array exceeding limit =====
    ["this is a long string element", "another long string value here", "yet another one that is quite long", "and one more string element here too"];

    // ===== Very long array far exceeding limit =====
    ["alpha", "bravo", "charlie", "delta", "echo", "foxtrot", "golf", "hotel", "india", "juliet", "kilo", "lima", "mike", "november", "oscar", "papa"];

    // ===== Nested arrays =====
    [[1, 2], [3, 4]];
    [[1, 2, 3], [4, 5, 6], [7, 8, 9]];

    // ===== Deeply nested arrays exceeding limit =====
    [[[1, 2], [3, 4]], [[5, 6], [7, 8]], [[9, 10], [11, 12]], [[13, 14], [15, 16]]];

    // ===== Trivia in array (line comments) =====
    // 1 open bracket leading
    [ // 2 open bracket trailing
    // 3 first element leading
    1 // 4 first element trailing
    , // 5 comma trailing
    // 6 second element leading
    2 // 7 second element trailing
    // 8 close bracket leading
    ] // 9 close bracket trailing
    ;

    // ===== Trivia in array (block comments) =====
    /* 1 open bracket leading */
    [ /* 2 open bracket trailing */
    /* 3 first element leading */
    1 /* 4 first element trailing */
    , /* 5 comma trailing */
    /* 6 second element leading */
    2 /* 7 second element trailing */
    /* 8 close bracket leading */
    ] /* 9 close bracket trailing */
    ;

    // ===== Trivia in long wrapping array (line comments) =====
    [ // 1 trailing
    // 2 leading
    "this is a long string" // 3 trailing
    , // 4 trailing
    // 5 leading
    "another long string value" // 6 trailing
    , // 7 trailing
    // 8 leading
    "yet another that is quite long" // 9 trailing
    , // 10 trailing
    // 11 leading
    "and one more string element" // 12 trailing
    ] // 13 trailing
    ;

    // ===== Trivia in long wrapping array (block comments) =====
    [ /* 1 trailing */
    /* 2 leading */
    "this is a long string" /* 3 trailing */
    , /* 4 trailing */
    /* 5 leading */
    "another long string value" /* 6 trailing */
    , /* 7 trailing */
    /* 8 leading */
    "yet another that is quite long" /* 9 trailing */
    , /* 10 trailing */
    /* 11 leading */
    "and one more string element" /* 12 trailing */
    ] /* 13 trailing */
    ;

    // ===== Map literals =====
    {"key": 1};
    {"a": 1, "b": 2};
    {"a": 1, "b": 2, "c": 3, "d": 4, "e": 5};

    // ===== Long map exceeding limit =====
    {"first_key": "first value", "second_key": "second value", "third_key": "third value", "fourth_key": "fourth value", "fifth_key": "fifth"};

    // ===== Nested map exceeding limit =====
    {"outer": {"inner_a": 1, "inner_b": 2, "inner_c": 3}, "other": {"inner_d": 4, "inner_e": 5, "inner_f": 6}};

    // ===== Trivia in map (line comments) =====
    // 1 open brace leading
    { // 2 open brace trailing
    // 3 key leading
    "k" // 4 key trailing
    : // 5 colon trailing
    // 6 value leading
    1 // 7 value trailing
    // 8 close brace leading
    } // 9 close brace trailing
    ;

    // ===== Trivia in map (block comments) =====
    /* 1 open brace leading */
    { /* 2 open brace trailing */
    /* 3 key leading */
    "k" /* 4 key trailing */
    : /* 5 colon trailing */
    /* 6 value leading */
    1 /* 7 value trailing */
    /* 8 close brace leading */
    } /* 9 close brace trailing */
    ;

    // ===== Object initializers =====
    OtherPoint { x: 1, y: 2 };
    OtherVec { p: OtherPoint { x: 1, y: 2 }, q: OtherPoint { x: 3, y: 4 } };

    // ===== Object initializer exceeding limit =====
    OtherVec { p: OtherPoint { x: 100000, y: 200000 }, q: OtherPoint { x: 300000, y: 400000 } };

    // ===== Deeply nested object exceeding limit =====
    OtherTriple { a: OtherVec { p: OtherPoint { x: 1, y: 2 }, q: OtherPoint { x: 3, y: 4 } }, b: OtherVec { p: OtherPoint { x: 5, y: 6 }, q: OtherPoint { x: 7, y: 8 } }, c: OtherVec { p: OtherPoint { x: 9, y: 10 }, q: OtherPoint { x: 11, y: 12 } } };

    // ===== Trivia in object initializer (line comments) =====
    // 1 type name leading
    OtherPoint // 2 type name trailing
    // 3 open brace leading
    { // 4 open brace trailing
    // 5 field name leading
    x // 6 field name trailing
    : // 7 colon trailing
    // 8 value leading
    1 // 9 value trailing
    , // 10 comma trailing
    // 11 second field leading
    y // 12 second name trailing
    : // 13 second colon trailing
    // 14 second value leading
    2 // 15 second value trailing
    // 16 close brace leading
    } // 17 close brace trailing
    ;

    // ===== Trivia in object initializer (block comments) =====
    /* 1 type name leading */
    OtherPoint /* 2 type name trailing */
    /* 3 open brace leading */
    { /* 4 open brace trailing */
    /* 5 field name leading */
    x /* 6 field name trailing */
    : /* 7 colon trailing */
    /* 8 value leading */
    1 /* 9 value trailing */
    , /* 10 comma trailing */
    /* 11 second field leading */
    y /* 12 second name trailing */
    : /* 13 second colon trailing */
    /* 14 second value leading */
    2 /* 15 second value trailing */
    /* 16 close brace leading */
    } /* 17 close brace trailing */
    ;

    // ===== Trivia in nested object exceeding limit (line comments) =====
    // 1 leading
    OtherVec // 2 trailing
    { // 3 trailing
    // 4 leading
    p // 5 trailing
    : // 6 trailing
    // 7 leading
    OtherPoint // 8 trailing
    { // 9 trailing
    // 10 leading
    x // 11 trailing
    : // 12 trailing
    // 13 leading
    100000 // 14 trailing
    , // 15 trailing
    // 16 leading
    y // 17 trailing
    : // 18 trailing
    // 19 leading
    200000 // 20 trailing
    } // 21 trailing
    , // 22 trailing
    // 23 leading
    q // 24 trailing
    : // 25 trailing
    // 26 leading
    OtherPoint // 27 trailing
    { // 28 trailing
    // 29 leading
    x // 30 trailing
    : // 31 trailing
    // 32 leading
    300000 // 33 trailing
    , // 34 trailing
    // 35 leading
    y // 36 trailing
    : // 37 trailing
    // 38 leading
    400000 // 39 trailing
    } // 40 trailing
    } // 41 trailing
    ;

    // ===== Block expressions =====
    {
        let x = 1;
        x + 1
    };

    {
        let x = a + b;
        let y = x * 2;
        y
    };

    // ===== Deeply nested block expressions =====
    {
        let x = {
            let y = {
                let z = a + b;
                z * z
            };
            y + y
        };
        x * x
    };

    // ===== Block with nested if/match =====
    {
        let x = if (a > b) {
            match (a) {
                0 => {
                    let inner = b * 2;
                    inner + 1
                },
                _ => a
            }
        } else {
            b
        };
        x
    };

    // ===== Extraneous whitespace =====
    [  1  ,  2  ,  3  ];
    {  "k"  :  1  };
    OtherPoint  {  x  :  1  ,  y  :  2  };

    // ===== Final expression =====
    a + b
}
