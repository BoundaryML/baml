// Testing all the formatter cases for loop statements
// both for and while loops

function LoopStmts(items: int[], flag: bool) -> int {

    // ===== Basic while loop =====
    let a = 0;
    while (a < 10) {
        a += 1;
    };

    // ===== While with complex condition =====
    while (a < 100 && flag) {
        a += 1;
    };

    // ===== While with break =====
    while (true) {
        if (a > 50) {
            break;
        }
        a += 1;
    };

    // ===== While with continue =====
    while (a < 100) {
        a += 1;
        if (a % 2 == 0) {
            continue;
        }
    };

    // ===== Trivia on while loop (line comments) =====
    // 1 while keyword leading
    while // 2 while keyword trailing
    // 3 condition leading
    ( // 4 open paren trailing
    // 5 expr leading
    a < 10 // 6 expr trailing
    // 7 close paren leading
    ) // 8 close paren trailing
    // 9 body leading
    { // 10 body open trailing
        a += 1;
    // 11 body close leading
    } // 12 body close trailing
    ;

    ///////////////////

    // ===== Trivia on while loop (block comments) =====
    /* 1 while keyword leading */
    while /* 2 while keyword trailing */
    /* 3 condition leading */
    ( /* 4 open paren trailing */
    /* 5 expr leading */
    a < 10 /* 6 expr trailing */
    /* 7 close paren leading */
    ) /* 8 close paren trailing */
    /* 9 body leading */
    { /* 10 body open trailing */
        a += 1;
    /* 11 body close leading */
    } /* 12 body close trailing */
    ;

    ///////////////////

    // ===== C-style for loop =====
    let result = 0;
    for (let i = 0; i < 10; i += 1) {
        result += i;
    };

    // ===== C-style for with complex expressions =====
    for (let i = 0; i < 100 && flag; i += 2) {
        result += i;
    };

    // ===== Iterator-style for loop =====
    for (let item in items) {
        result += item;
    };

    // ===== For-in with array literal =====
    for (let x in [1, 2, 3, 4, 5]) {
        result += x;
    };

    // ===== Nested for loops =====
    for (let i in [1, 2, 3]) {
        for (let j in [4, 5, 6]) {
            result += i * j;
        };
    };

    // ===== Trivia on C-style for (line comments) =====
    // 1 for keyword leading
    for // 2 for keyword trailing
    // 3 open paren leading
    ( // 4 open paren trailing
    // 5 init leading
    let // 6 let trailing
    // 7 var name leading
    i // 8 var name trailing
    = // 9 equals trailing
    // 10 init value leading
    0 // 11 init value trailing
    ; // 12 semi1 trailing
    // 13 condition leading
    i < 10 // 14 condition trailing
    ; // 15 semi2 trailing
    // 16 update leading
    i += 1 // 17 update trailing
    // 18 close paren leading
    ) // 19 close paren trailing
    // 20 body leading
    { // 21 body open trailing
        result += i;
    // 22 body close leading
    } // 23 body close trailing
    ;

    ///////////////////

    // ===== Trivia on C-style for (block comments) =====
    /* 1 for keyword leading */
    for /* 2 for keyword trailing */
    /* 3 open paren leading */
    ( /* 4 open paren trailing */
    /* 5 init leading */
    let /* 6 let trailing */
    /* 7 var name leading */
    i /* 8 var name trailing */
    = /* 9 equals trailing */
    /* 10 init value leading */
    0 /* 11 init value trailing */
    ; /* 12 semi1 trailing */
    /* 13 condition leading */
    i < 10 /* 14 condition trailing */
    ; /* 15 semi2 trailing */
    /* 16 update leading */
    i += 1 /* 17 update trailing */
    /* 18 close paren leading */
    ) /* 19 close paren trailing */
    /* 20 body leading */
    { /* 21 body open trailing */
        result += i;
    /* 22 body close leading */
    } /* 23 body close trailing */
    ;

    ///////////////////

    // ===== Trivia on for-in (line comments) =====
    // 1 for keyword leading
    for // 2 for keyword trailing
    // 3 open paren leading
    ( // 4 open paren trailing
    // 5 let leading
    let // 6 let trailing
    // 7 var name leading
    item // 8 var name trailing
    // 9 in leading
    in // 10 in trailing
    // 11 iterator leading
    items // 12 iterator trailing
    // 13 close paren leading
    ) // 14 close paren trailing
    // 15 body leading
    { // 16 body open trailing
        result += item;
    // 17 body close leading
    } // 18 body close trailing
    ;

    // ===== Trivia on for-in (block comments) =====
    /* 1 for keyword leading */
    for /* 2 for keyword trailing */
    /* 3 open paren leading */
    ( /* 4 open paren trailing */
    /* 5 let leading */
    let /* 6 let trailing */
    /* 7 var name leading */
    item /* 8 var name trailing */
    /* 9 in leading */
    in /* 10 in trailing */
    /* 11 iterator leading */
    items /* 12 iterator trailing */
    /* 13 close paren leading */
    ) /* 14 close paren trailing */
    /* 15 body leading */
    { /* 16 body open trailing */
        result += item;
    /* 17 body close leading */
    } /* 18 body close trailing */
    ;

    // ===== Long while condition exceeding limit =====
    while (a < 100 && result > 0 && flag && a != result && a + result < 10000 && a * result > 500 && a - result != 42) {
        a += 1;
    };

    // ===== Very long while condition far exceeding limit =====
    while (a < 100 && result > 0 && flag && a != result && a + result < 10000 && a * result > 500 && a - result != 42 && a % 7 == 0 && result % 3 == 0 && a + result + a * result > 99999) {
        a += 1;
    };

    // ===== Long C-style for exceeding limit =====
    for (let very_long_variable_name = 0; very_long_variable_name < 1000 && flag; very_long_variable_name += 1) {
        result += very_long_variable_name;
    };

    // ===== Very long C-style for far exceeding limit =====
    for (let extremely_long_loop_counter_variable = a * a + result * result; extremely_long_loop_counter_variable < 1000000 && flag && a > 0; extremely_long_loop_counter_variable += a + result) {
        result += extremely_long_loop_counter_variable;
    };

    // ===== Long for-in with long array exceeding limit =====
    for (let item in ["very long string one", "very long string two", "very long string three", "very long string four"]) {
        result += 1;
    };

    // ===== Very long for-in with complex iterator exceeding limit =====
    for (let extremely_long_item_variable_name in ["alpha bravo charlie", "delta echo foxtrot", "golf hotel india", "juliet kilo lima", "mike november oscar", "papa quebec romeo"]) {
        result += 1;
    };

    // ===== Trivia on wrapping while condition (line comments) =====
    // 1 while leading
    while // 2 while trailing
    // 3 open paren leading
    ( // 4 open paren trailing
    // 5 condition leading
    a < 100 && result > 0 && flag && a != result && a + result < 10000 && a * result > 500 && a - result != 42 // 6 condition trailing
    // 7 close paren leading
    ) // 8 close paren trailing
    // 9 body leading
    { // 10 body open trailing
        a += 1;
    // 11 close leading
    } // 12 close trailing
    ;

    ///////////////////

    // ===== Trivia on wrapping while condition (block comments) =====
    /* 1 while leading */
    while /* 2 while trailing */
    /* 3 open paren leading */
    ( /* 4 open paren trailing */
    /* 5 condition leading */
    a < 100 && result > 0 && flag && a != result && a + result < 10000 && a * result > 500 && a - result != 42 /* 6 condition trailing */
    /* 7 close paren leading */
    ) /* 8 close paren trailing */
    /* 9 body leading */
    { /* 10 body open trailing */
        a += 1;
    /* 11 close leading */
    } /* 12 close trailing */
    ;

    ///////////////////

    // ===== Trivia on wrapping C-style for (line comments) =====
    // 1 for leading
    for // 2 for trailing
    // 3 open paren leading
    ( // 4 open paren trailing
    // 5 init leading
    let // 6 let trailing
    // 7 name leading
    extremely_long_loop_counter_variable // 8 name trailing
    = // 9 equals trailing
    // 10 init value leading
    a * a + result * result // 11 init value trailing
    ; // 12 semi1 trailing
    // 13 condition leading
    extremely_long_loop_counter_variable < 1000000 && flag && a > 0 // 14 condition trailing
    ; // 15 semi2 trailing
    // 16 update leading
    extremely_long_loop_counter_variable += a + result // 17 update trailing
    // 18 close paren leading
    ) // 19 close paren trailing
    // 20 body leading
    { // 21 body open trailing
        result += extremely_long_loop_counter_variable;
    // 22 close leading
    } // 23 close trailing
    ;

    ///////////////////

    // ===== Trivia on wrapping C-style for (block comments) =====
    /* 1 for leading */
    for /* 2 for trailing */
    /* 3 open paren leading */
    ( /* 4 open paren trailing */
    /* 5 init leading */
    let /* 6 let trailing */
    /* 7 name leading */
    extremely_long_loop_counter_variable /* 8 name trailing */
    = /* 9 equals trailing */
    /* 10 init value leading */
    a * a + result * result /* 11 init value trailing */
    ; /* 12 semi1 trailing */
    /* 13 condition leading */
    extremely_long_loop_counter_variable < 1000000 && flag && a > 0 /* 14 condition trailing */
    ; /* 15 semi2 trailing */
    /* 16 update leading */
    extremely_long_loop_counter_variable += a + result /* 17 update trailing */
    /* 18 close paren leading */
    ) /* 19 close paren trailing */
    /* 20 body leading */
    { /* 21 body open trailing */
        result += extremely_long_loop_counter_variable;
    /* 22 close leading */
    } /* 23 close trailing */
    ;

    ///////////////////

    // ===== Trivia on wrapping for-in (line comments) =====
    // 1 for leading
    for // 2 for trailing
    // 3 open paren leading
    ( // 4 open paren trailing
    // 5 let leading
    let // 6 let trailing
    // 7 name leading
    extremely_long_item_variable_name // 8 name trailing
    // 9 in leading
    in // 10 in trailing
    // 11 iterator leading
    ["alpha bravo charlie", "delta echo foxtrot", "golf hotel india", "juliet kilo lima", "mike november oscar", "papa quebec romeo"] // 12 iterator trailing
    // 13 close paren leading
    ) // 14 close paren trailing
    // 15 body leading
    { // 16 body open trailing
        result += 1;
    // 17 close leading
    } // 18 close trailing
    ;

    ///////////////////

    // ===== Trivia on wrapping for-in (block comments) =====
    /* 1 for leading */
    for /* 2 for trailing */
    /* 3 open paren leading */
    ( /* 4 open paren trailing */
    /* 5 let leading */
    let /* 6 let trailing */
    /* 7 name leading */
    extremely_long_item_variable_name /* 8 name trailing */
    /* 9 in leading */
    in /* 10 in trailing */
    /* 11 iterator leading */
    ["alpha bravo charlie", "delta echo foxtrot", "golf hotel india", "juliet kilo lima", "mike november oscar", "papa quebec romeo"] /* 12 iterator trailing */
    /* 13 close paren leading */
    ) /* 14 close paren trailing */
    /* 15 body leading */
    { /* 16 body open trailing */
        result += 1;
    /* 17 close leading */
    } /* 18 close trailing */
    ;

    ///////////////////

    // ===== Deeply nested loops (3 levels) =====
    for (let i in [1, 2, 3]) {
        for (let j in [4, 5, 6]) {
            for (let k in [7, 8, 9]) {
                result += i * j * k;
            };
        };
    };

    // ===== Deeply nested while loops (3 levels) =====
    let outer = 0;
    while (outer < 3) {
        let middle = 0;
        while (middle < 3) {
            let inner = 0;
            while (inner < 3) {
                result += outer + middle + inner;
                inner += 1;
            };
            middle += 1;
        };
        outer += 1;
    };

    // ===== Deeply nested loops with break/continue/match =====
    for (let i in [1, 2, 3, 4, 5]) {
        if (i == 3) {
            continue;
        }
        for (let j in [10, 20, 30, 40, 50]) {
            let val = match (i * j) {
                0 => {
                    break;
                    0
                },
                n: int if n > 100 => {
                    continue;
                    n
                },
                n: int => n
            };
            result += val;
        };
    };

    // ===== Very deeply nested (4 levels) with mixed loop types =====
    let lvl1 = 0;
    while (lvl1 < 2) {
        for (let lvl2 in [1, 2]) {
            for (let lvl3 = 0; lvl3 < 2; lvl3 += 1) {
                let lvl4 = 0;
                while (lvl4 < 2) {
                    result += lvl1 + lvl2 + lvl3 + lvl4;
                    lvl4 += 1;
                };
            };
        };
        lvl1 += 1;
    };

    // ===== Nested loop with long inner condition exceeding limit =====
    for (let i in items) {
        for (let j = i * i + result * result; j < 1000000 && flag && i > 0 && result > 0 && j != 42; j += i + result + 1) {
            result += j;
        };
    };

    // ===== Deeply nested with if/match in body exceeding limit =====
    for (let i in [1, 2, 3, 4, 5, 6, 7, 8, 9, 10]) {
        for (let j in [10, 20, 30, 40, 50, 60, 70, 80, 90, 100]) {
            let computation = if (i * j > 500 && i + j < 100) {
                match (i % 3) {
                    0 => i * j + result,
                    1 => i * j - result,
                    _ => i * j
                }
            } else {
                0
            };
            result += computation;
        };
    };

    // ===== Extraneous whitespace =====
    while    (  a  <  10  )    {
        a += 1;
    };

    for    (  let  i  =  0  ;  i  <  10  ;  i  +=  1  )    {
        result += i;
    };

    for    (  let  item  in  items  )    {
        result += item;
    };

    // ===== Let statement variations =====
    let plain = 1;
    let typed: int = 2;
    let complex_init = a + result;

    // ===== Long let with type annotation exceeding limit =====
    let very_long_variable_name_for_testing_wrapping: map<string, int | string | bool | float | null> = {"key": 42};

    // ===== Very long let with expression exceeding limit =====
    let another_extremely_long_variable_name = a * a + result * result + a * result + a + result + a * a * a + result * result * result + a * result * a + result * a * result;

    // ===== Long let with nested generic type exceeding limit =====
    let deeply_typed: map<string, map<string, map<string, int | string | bool>>> = {"outer": {"inner": {"deep": 42}}};

    // ===== Trivia on let statement (line comments) =====
    // 1 let keyword leading
    let // 2 let keyword trailing
    // 3 name leading
    trivia_var // 4 name trailing
    // 5 colon leading
    : // 6 colon trailing
    // 7 type leading
    int // 8 type trailing
    // 9 equals leading
    = // 10 equals trailing
    // 11 value leading
    42 // 12 value trailing
    ; // 13 semicolon trailing

    // ===== Trivia on let statement (block comments) =====
    /* 1 let keyword leading */
    let /* 2 let keyword trailing */
    /* 3 name leading */
    trivia_var_block /* 4 name trailing */
    /* 5 colon leading */
    : /* 6 colon trailing */
    /* 7 type leading */
    int /* 8 type trailing */
    /* 9 equals leading */
    = /* 10 equals trailing */
    /* 11 value leading */
    42 /* 12 value trailing */
    ; /* 13 semicolon trailing */

    // ===== Trivia on wrapping let with long type (line comments) =====
    // 1 let leading
    let // 2 let trailing
    // 3 name leading
    very_long_variable_name_for_testing_wrapping // 4 name trailing
    // 5 colon leading
    : // 6 colon trailing
    // 7 type leading
    map<string, int | string | bool | float | null> // 8 type trailing
    // 9 equals leading
    = // 10 equals trailing
    // 11 value leading
    {"key": 42} // 12 value trailing
    ; // 13 semicolon trailing

    ///////////////////

    // ===== Trivia on wrapping let with long type (block comments) =====
    /* 1 let leading */
    let /* 2 let trailing */
    /* 3 name leading */
    very_long_variable_name_for_testing_wrapping /* 4 name trailing */
    /* 5 colon leading */
    : /* 6 colon trailing */
    /* 7 type leading */
    map<string, int | string | bool | float | null> /* 8 type trailing */
    /* 9 equals leading */
    = /* 10 equals trailing */
    /* 11 value leading */
    {"key": 42} /* 12 value trailing */
    ; /* 13 semicolon trailing */

    ///////////////////

    // ===== Trivia on wrapping let with long expression (line comments) =====
    // 1 let leading
    let // 2 let trailing
    // 3 name leading
    another_extremely_long_variable_name // 4 name trailing
    // 5 equals leading
    = // 6 equals trailing
    // 7 value leading
    a * a + result * result + a * result + a + result + a * a * a + result * result * result + a * result * a + result * a * result // 8 value trailing
    ; // 9 semicolon trailing

    ///////////////////

    // ===== Trivia on wrapping let with long expression (block comments) =====
    /* 1 let leading */
    let /* 2 let trailing */
    /* 3 name leading */
    another_extremely_long_variable_name /* 4 name trailing */
    /* 5 equals leading */
    = /* 6 equals trailing */
    /* 7 value leading */
    a * a + result * result + a * result + a + result + a * a * a + result * result * result + a * result * a + result * a * result /* 8 value trailing */
    ; /* 9 semicolon trailing */

    ///////////////////

    // ===== Return statements =====
    return result;
}

// ===== Assert statements =====
function AssertStmts(x: int, y: int) -> int {
    assert x > 0;
    assert x < 100;

    // ===== Trivia on assert (line comments) =====
    // 1 assert keyword leading
    assert // 2 assert keyword trailing
    // 3 condition leading
    x > 0 // 4 condition trailing
    ; // 5 semicolon trailing

    // ===== Trivia on assert (block comments) =====
    /* 1 assert keyword leading */
    assert /* 2 assert keyword trailing */
    /* 3 condition leading */
    x > 0 /* 4 condition trailing */
    ; /* 5 semicolon trailing */

    // ===== Long assert condition exceeding limit =====
    assert x > 0 && x < 100 && y > 0 && y < 100 && x != y && x + y < 150 && x * y > 10 && x - y != 42 && x % 7 == 0;

    // ===== Very long assert condition far exceeding limit =====
    assert x > 0 && x < 100 && y > 0 && y < 100 && x != y && x + y < 150 && x * y > 10 && x - y != 42 && x % 7 == 0 && y % 3 == 0 && x + y + x * y > 100 && x * x + y * y < 10000;

    // ===== Trivia on wrapping assert (line comments) =====
    // 1 assert leading
    assert // 2 assert trailing
    // 3 condition leading
    x > 0 && x < 100 && y > 0 && y < 100 && x != y && x + y < 150 && x * y > 10 && x - y != 42 && x % 7 == 0 // 4 condition trailing
    ; // 5 semicolon trailing

    ///////////////////

    // ===== Trivia on wrapping assert (block comments) =====
    /* 1 assert leading */
    assert /* 2 assert trailing */
    /* 3 condition leading */
    x > 0 && x < 100 && y > 0 && y < 100 && x != y && x + y < 150 && x * y > 10 && x - y != 42 && x % 7 == 0 /* 4 condition trailing */
    ; /* 5 semicolon trailing */

    ///////////////////

    x
}
