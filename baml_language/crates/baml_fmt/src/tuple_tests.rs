use crate::{FormatOptions, format};

/// Formats `source`, asserts the result is a fixed point, and returns it.
fn fmt(source: &str) -> String {
    let options = FormatOptions::default();
    let formatted = format(source, &options)
        .unwrap_or_else(|e| panic!("source should format: {e:?}\nsource:\n{source}"));
    let second = format(&formatted, &options)
        .unwrap_or_else(|e| panic!("formatted output should format: {e:?}\n{formatted}"));
    assert_eq!(formatted, second, "formatter should be idempotent");
    formatted
}

/// Asserts `source` is already formatted.
fn assert_fixed_point(source: &str) {
    assert_eq!(fmt(source), source);
}

#[test]
fn tuple_literals_are_fixed_points() {
    assert_fixed_point(concat!(
        "function f() -> int {\n",
        "    let a = (1, \"a\");\n",
        "    let b = (1,);\n",
        "    let c = ((1, 2), (3,), [4]);\n",
        "    let d = (a, b)[0];\n",
        "    0\n",
        "}\n",
    ));
}

#[test]
fn tuple_literal_spacing_and_trailing_comma_normalize() {
    let formatted = fmt(concat!(
        "function f() -> int {\n",
        "    let a = ( 1 ,\"a\" , );\n",
        "    let b = ( 1 , );\n",
        "    0\n",
        "}\n",
    ));
    assert!(formatted.contains("let a = (1, \"a\");"), "{formatted}");
    assert!(formatted.contains("let b = (1,);"), "{formatted}");
}

#[test]
fn one_tuple_keeps_its_comma_and_is_not_peeled() {
    // As a call argument, a transparent paren peels; a 1-tuple must not.
    let formatted = fmt(concat!(
        "function f() -> int {\n",
        "    g((1,), ((2,)), ((3)));\n",
        "    0\n",
        "}\n",
    ));
    assert!(formatted.contains("g((1,), (2,), 3);"), "{formatted}");
}

#[test]
fn long_tuple_breaks_one_element_per_line() {
    let formatted = fmt(concat!(
        "function f() -> int {\n",
        "    let t = (some_long_identifier_number_one, some_long_identifier_number_two, some_long_identifier_number_three);\n",
        "    0\n",
        "}\n",
    ));
    assert_eq!(
        formatted,
        concat!(
            "function f() -> int {\n",
            "    let t = (\n",
            "        some_long_identifier_number_one,\n",
            "        some_long_identifier_number_two,\n",
            "        some_long_identifier_number_three,\n",
            "    );\n",
            "    0\n",
            "}\n",
        )
    );
}

/// Like a 1-element array, the 1-tuple breaks once its own width exceeds the
/// line width (the `let t = ` prefix is not counted, matching arrays).
#[test]
fn long_one_tuple_breaks_with_trailing_comma() {
    let formatted = fmt(concat!(
        "function f() -> int {\n",
        "    let t = (some_function_with_a_very_long_name(first_argument_value, second_argument_value, third_argument_value),);\n",
        "    0\n",
        "}\n",
    ));
    assert!(
        formatted.contains(concat!(
            "    let t = (\n",
            "        some_function_with_a_very_long_name(first_argument_value, second_argument_value, third_argument_value),\n",
            "    );\n",
        )),
        "{formatted}"
    );
}

#[test]
fn tuple_dot_access_round_trips() {
    assert_fixed_point("function f(t: ((int, string), bool)) -> string {\n    t.0.1\n}\n");
    assert_fixed_point("function f(t: (int, string)?) -> string? {\n    t?.1\n}\n");
}

#[test]
fn tuple_function_calls_keep_parentheses() {
    let source = "function f(t: ((int) -> int,)) -> int {\n    (t.0)(3)\n}\n";
    let formatted = fmt(source);
    assert!(formatted.contains("(t.0)(3)"), "{formatted}");
    let source = "function f(v: Holder) -> int {\n    (v.f)(3)\n}\n";
    let formatted = fmt(source);
    assert!(formatted.contains("(v.f)(3)"), "{formatted}");
}

#[test]
fn tuple_types_are_fixed_points() {
    assert_fixed_point(concat!(
        "function f(\n",
        "    a: (int, string),\n",
        "    b: (int,),\n",
        "    c: (int, string)[],\n",
        "    d: (int, (bool, float))?,\n",
        "    e: map<string, (int, int)>,\n",
        "    g: string | (int, string),\n",
        "    h: (int, string) -> bool,\n",
        "    i: ((int, string)) -> bool,\n",
        "    j: ((int, string), bool) -> (int,),\n",
        ") -> (int, string) {\n",
        "    (1, \"a\")\n",
        "}\n",
    ));
}

#[test]
fn tuple_types_normalize() {
    let formatted = fmt(concat!(
        "type A = ( int , string , )\n",
        "type B = ( int , )\n",
        "type C = (int,string)[]|null\n",
        "type D = ((int,string))->bool\n",
    ));
    assert_eq!(
        formatted,
        concat!(
            "type A = (int, string);\n\n",
            "type B = (int,);\n\n",
            "type C = (int, string)[] | null;\n\n",
            "type D = ((int, string)) -> bool;\n",
        )
    );
}

#[test]
fn long_tuple_type_breaks_one_element_per_line() {
    let formatted = fmt(
        "type T = (SomeVeryLongTypeNameNumberOne, SomeVeryLongTypeNameNumberTwo, SomeVeryLongTypeNameNumberThree)\n",
    );
    assert_eq!(
        formatted,
        concat!(
            "type T = (\n",
            "    SomeVeryLongTypeNameNumberOne,\n",
            "    SomeVeryLongTypeNameNumberTwo,\n",
            "    SomeVeryLongTypeNameNumberThree,\n",
            ");\n",
        )
    );
}

#[test]
fn tuple_patterns_are_fixed_points() {
    assert_fixed_point(concat!(
        "function f(t: (int, string)) -> int {\n",
        "    let (let a, let b) = t;\n",
        "    let (let c,) = (1,);\n",
        "    match (t) {\n",
        "        (0, _) => 0,\n",
        "        (let x, \"a\" | \"b\") => x,\n",
        "        ((1, 2), let y) => 1,\n",
        "        (let z,) => z,\n",
        "        _ => 2,\n",
        "    }\n",
        "}\n",
    ));
}

#[test]
fn tuple_patterns_normalize() {
    let formatted = fmt(concat!(
        "function f(t: (int, string)) -> int {\n",
        "    let ( let a , let b , ) = t;\n",
        "    let ( let c , ) = (1,);\n",
        "    match (t) {\n",
        "        ( 0 , _ ) => 0,\n",
        "        _ => 2,\n",
        "    }\n",
        "}\n",
    ));
    assert!(formatted.contains("let (let a, let b) = t;"), "{formatted}");
    assert!(formatted.contains("let (let c,) = (1,);"), "{formatted}");
    assert!(formatted.contains("(0, _) => 0,"), "{formatted}");
}

#[test]
fn match_on_tuple_scrutinee() {
    assert_fixed_point(concat!(
        "function f(a: bool, b: bool) -> int {\n",
        "    match (a, b) {\n",
        "        (true, true) => 1,\n",
        "        (false, _) | (_, false) => 0,\n",
        "    }\n",
        "}\n",
    ));
    let formatted = fmt(concat!(
        "function f(a: bool) -> int {\n",
        "    match ( a , ) {\n",
        "        (true,) => 1,\n",
        "        _ => 0,\n",
        "    }\n",
        "}\n",
    ));
    assert!(formatted.contains("    match (a,) {\n"), "{formatted}");
}

/// The scrutinee annotation needs its own parens around a tuple scrutinee,
/// `match ((a, b): T)`, and keeps them.
#[test]
fn annotated_match_scrutinee() {
    assert_fixed_point(concat!(
        "function f(a: int, b: string?) -> int {\n",
        "    match ((a, b): (int, string?)) {\n",
        "        (0, null) => 0,\n",
        "        _ => 1,\n",
        "    }\n",
        "}\n",
    ));
    let formatted = fmt(concat!(
        "function f(x: int) -> int {\n",
        "    match ( x :int ) {\n",
        "        _ => 1,\n",
        "    }\n",
        "}\n",
    ));
    assert!(formatted.contains("    match (x: int) {\n"), "{formatted}");
}

#[test]
fn long_match_tuple_scrutinee_breaks() {
    let formatted = fmt(concat!(
        "function f() -> int {\n",
        "    match (some_long_scrutinee_number_one, some_long_scrutinee_number_two, some_long_scrutinee_number_three) {\n",
        "        _ => 0,\n",
        "    }\n",
        "}\n",
    ));
    assert_eq!(
        formatted,
        concat!(
            "function f() -> int {\n",
            "    match (\n",
            "        some_long_scrutinee_number_one,\n",
            "        some_long_scrutinee_number_two,\n",
            "        some_long_scrutinee_number_three,\n",
            "    ) {\n",
            "        _ => 0,\n",
            "    }\n",
            "}\n",
        )
    );
}

#[test]
fn tuple_comments_are_preserved() {
    let formatted = fmt(concat!(
        "function f() -> int {\n",
        "    let t = (\n",
        "        1, // one\n",
        "        2, // two\n",
        "    );\n",
        "    0\n",
        "}\n",
    ));
    assert!(formatted.contains("1, // one"), "{formatted}");
    assert!(formatted.contains("2, // two"), "{formatted}");
}

#[test]
fn tuple_patterns_in_refutable_heads_are_fixed_points() {
    assert_fixed_point(concat!(
        "function f(t: (int, string)?, ts: (int, string)[], us: (int,)[]) -> int {\n",
        "    if let (let a, \"x\") = t {\n",
        "        a\n",
        "    } else if let (let b,) = (1,) {\n",
        "        b\n",
        "    } else {\n",
        "        0\n",
        "    }\n",
        "    if const (const c, _) = t {\n",
        "        c\n",
        "    } else {\n",
        "        0\n",
        "    }\n",
        "    while let (let d, _) = t {\n",
        "        break;\n",
        "    }\n",
        "    while let (let e,) = (1,) {\n",
        "        break;\n",
        "    }\n",
        "    let (let g, _) = t else {\n",
        "        return 0;\n",
        "    };\n",
        "    let (0,) = (g,) else {\n",
        "        return 1;\n",
        "    };\n",
        "    for (let (let h, let i) in ts) {\n",
        "        h;\n",
        "    }\n",
        "    for (const (const j,) in us) {\n",
        "        j;\n",
        "    }\n",
        "    g\n",
        "}\n",
    ));
}

#[test]
fn tuple_patterns_in_refutable_heads_normalize() {
    let formatted = fmt(concat!(
        "function f(t: (int, string)?, ts: (int, string)[]) -> int {\n",
        "    if let ( let a , _ , ) = t { a } else { 0 }\n",
        "    if const ( const b , ) = (1,) { b } else { 0 }\n",
        "    while let ( let c , ) = (1,) { break; }\n",
        "    let ( let d , _ ) = t else { return 0; };\n",
        "    for (let ( let e , let g ) in ts) { e; }\n",
        "    d\n",
        "}\n",
    ));
    assert!(
        formatted.contains("    if let (let a, _) = t {\n"),
        "{formatted}"
    );
    assert!(
        formatted.contains("    if const (const b,) = (1,) {\n"),
        "{formatted}"
    );
    assert!(
        formatted.contains("    while let (let c,) = (1,) {\n"),
        "{formatted}"
    );
    assert!(
        formatted.contains("    let (let d, _) = t else {\n"),
        "{formatted}"
    );
    assert!(
        formatted.contains("    for (let (let e, let g) in ts) {\n"),
        "{formatted}"
    );
}
