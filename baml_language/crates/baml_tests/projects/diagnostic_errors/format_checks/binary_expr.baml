// Testing all the formatter cases for binary expressions
// including chains and nested chains

function BinaryExprs(a: int, b: int, c: int, d: int, s: string, flag: bool) -> int {

    // ===== Basic arithmetic =====
    a + b;
    a - b;
    a * b;
    a / b;
    a % b;

    // ===== Comparison operators =====
    a == b;
    a != b;
    a < b;
    a > b;
    a <= b;
    a >= b;

    // ===== Logical operators =====
    flag && true;
    flag || false;

    // ===== Bitwise operators =====
    a & b;
    a | b;
    a ^ b;
    a << b;
    a >> b;

    // ===== Assignment operators =====
    let mut_a = a;
    mut_a += b;
    mut_a -= b;
    mut_a *= b;
    mut_a /= b;
    mut_a %= b;
    mut_a &= b;
    mut_a |= b;
    mut_a ^= b;
    mut_a <<= b;
    mut_a >>= b;

    // ===== Chained arithmetic (same precedence group) =====
    a + b + c;
    a + b + c + d;
    a - b + c - d;
    a * b * c;
    a * b / c % d;

    // ===== Chained comparisons with logical =====
    a > 0 && b > 0;
    a > 0 || b > 0;
    a > 0 && b > 0 && c > 0;
    a > 0 || b > 0 || c > 0;
    a > 0 && b > 0 || c > 0;

    // ===== Mixed precedence =====
    a + b * c;
    (a + b) * c;
    a * b + c * d;
    a + b * c + d;

    // ===== Nested parenthesized =====
    (a + b) * (c + d);
    ((a + b) * c) + d;
    a + (b * (c + d));

    // ===== Deeply nested parens =====
    ((((a + b) * c) + d) * (((a - b) + c) - d)) + ((a * b) + (c * d));
    (((a + b) * (c + d)) + ((a - b) * (c - d))) * (((a * c * d) + (b * c * d)) - ((a * d) + (b * c * d)));

    // ===== Trivia between operands (line comments) =====
    // 1 left leading
    a // 2 left trailing
    // 3 operator leading
    + // 4 operator trailing
    // 5 right leading
    b // 6 right trailing
    ;

    // ===== Trivia between operands (block comments) =====
    /* 1 left leading */
    a /* 2 left trailing */
    /* 3 operator leading */
    + /* 4 operator trailing */
    /* 5 right leading */
    b /* 6 right trailing */
    ;

    // ===== Trivia in chained binary (line comments) =====
    // 1 first leading
    a // 2 first trailing
    // 3 first op leading
    + // 4 first op trailing
    // 5 second leading
    b // 6 second trailing
    // 7 second op leading
    + // 8 second op trailing
    // 9 third leading
    c // 10 third trailing
    ;

    // ===== Long arithmetic chain exceeding limit =====
    a + b + c + d + a + b + c + d + a + b + c + d + a + b + c + d + a + b + c + d + a + b + c + d + a + b;

    // ===== Long logical chain exceeding limit =====
    a > 0 && b > 0 && c > 0 && d > 0 && a != b && b != c && c != d && a < 1000 * 8 + 1 * 2 && b < 1000 && c < 1000 && d < 1000 + 2 + 3;

    // ===== Long mixed precedence chain exceeding limit =====
    a * b + c * d - a * b + c * d - a * b + c * d - a * b + c * d - a * b + c * d - a * b + c * d - a * b + c * d - a * b + c * d - a * b + c * d - a * b + c * d;

    // ===== Long comparison chain exceeding limit =====
    a + b + c + d > 0 && a * b * c * d < 10000 && a - b - c - d != 0 && (a + b) * (c + d) >= 100 || a * 1 < 2 && "aowiefjowiaj" == "jfeiowajpwoeifj";

    // ===== Long string concatenation exceeding limit =====
    "a very long string prefix that is already quite long" + " concatenated with " + "another very long string that together exceeds the line limit easily and comfortably";

    // ===== Long assignment expression exceeding limit =====
    mut_a += a * b + c * d + a * b + c * d + a * b + c * d + a * b + c * d + a * b + c * d + a;

    // ===== Trivia in long wrapping chain (line comments) =====
    // 1 leading
    a // 2 trailing
    // 3 leading
    + // 4 trailing
    // 5 leading
    b // 6 trailing
    // 7 leading
    + // 8 trailing
    // 9 leading
    c // 10 trailing
    // 11 leading
    + // 12 trailing
    // 13 leading
    d // 14 trailing
    // 15 leading
    + // 16 trailing
    // 17 leading
    a // 18 trailing
    // 19 leading
    + // 20 trailing
    // 21 leading
    b // 22 trailing
    // 23 leading
    + // 24 trailing
    // 25 leading
    c // 26 trailing
    // 27 leading
    + // 28 trailing
    // 29 leading
    d // 30 trailing
    // 31 leading
    + // 32 trailing
    // 33 leading
    a // 34 trailing
    // 35 leading
    + // 36 trailing
    // 37 leading
    b // 38 trailing
    // 39 leading
    + // 40 trailing
    // 41 leading
    c // 42 trailing
    // 43 leading
    + // 44 trailing
    // 45 leading
    d // 46 trailing
    ;

    // ===== Trivia in long wrapping chain (block comments) =====
    /* 1 leading */
    a /* 2 trailing */
    /* 3 leading */
    + /* 4 trailing */
    /* 5 leading */
    b /* 6 trailing */
    /* 7 leading */
    + /* 8 trailing */
    /* 9 leading */
    c /* 10 trailing */
    /* 11 leading */
    + /* 12 trailing */
    /* 13 leading */
    d /* 14 trailing */
    /* 15 leading */
    + /* 16 trailing */
    /* 17 leading */
    a /* 18 trailing */
    /* 19 leading */
    + /* 20 trailing */
    /* 21 leading */
    b /* 22 trailing */
    /* 23 leading */
    + /* 24 trailing */
    /* 25 leading */
    c /* 26 trailing */
    /* 27 leading */
    + /* 28 trailing */
    /* 29 leading */
    d /* 30 trailing */
    /* 31 leading */
    + /* 32 trailing */
    /* 33 leading */
    a /* 34 trailing */
    /* 35 leading */
    + /* 36 trailing */
    /* 37 leading */
    b /* 38 trailing */
    /* 39 leading */
    + /* 40 trailing */
    /* 41 leading */
    c /* 42 trailing */
    /* 43 leading */
    + /* 44 trailing */
    /* 45 leading */
    d /* 46 trailing */
    ;

    // ===== Trivia in long wrapping logical chain (line comments) =====
    // 1 leading
    a > 0 // 2 trailing
    // 3 leading
    && // 4 trailing
    // 5 leading
    b > 0 // 6 trailing
    // 7 leading
    && // 8 trailing
    // 9 leading
    c > 0 // 10 trailing
    // 11 leading
    && // 12 trailing
    // 13 leading
    d > 0 // 14 trailing
    // 15 leading
    && // 16 trailing
    // 17 leading
    a != b // 18 trailing
    // 19 leading
    && // 20 trailing
    // 21 leading
    b != c // 22 trailing
    // 23 leading
    && // 24 trailing
    // 25 leading
    c != d // 26 trailing
    ;

    // ===== Short expressions (should stay single line) =====
    a + b;
    1 + 2;
    a > b;

    // ===== Extraneous whitespace =====
    a   +   b;
    a    *    b    +    c;
    a     ==     b;
    a == 2       &&       b != 3;

    // ===== Final expression =====
    a + b
}
