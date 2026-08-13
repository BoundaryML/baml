//# Case 1
function IfThenOnlyHeader() -> int {
    let x = 1;

    //# Before

    //# If
    let y = if (false) { 1 };

    //## After
    y
}

//# Case 2
function IfThenOnlyHeader2() -> int {
    let x = 1;
    //## Before

    let z = 5;

    //### If
    let y = if (false) {
        //## Then
        1
    };

    //## After
    y
}


//# Case 3
function IfThenOnlyHeader2() -> int {
    let x = 1;
    //### Before

    let z = 5;

    //# If
    let y = if (false) {
        //## Then
        1
    };

    //## After
    y
}

//----
//- diagnostics
// E0010
//
//   × invalid syntax
//    ╭─[headers_if_then_only_header.baml:1:1]
//  1 │ //# Case 1
//    · ─────┬────
//    ·      ╰── header comments (`//#`) are only allowed in expression functions
//    ╰────
// E0010
//
//   × invalid syntax
//     ╭─[headers_if_then_only_header.baml:14:1]
//  14 │ //# Case 2
//     · ─────┬────
//     ·      ╰── header comments (`//#`) are only allowed in expression functions
//     ╰────
// E0010
//
//   × invalid syntax
//     ╭─[headers_if_then_only_header.baml:32:1]
//  32 │ //# Case 3
//     · ─────┬────
//     ·      ╰── header comments (`//#`) are only allowed in expression functions
//     ╰────
// E0001
//
//   × `if` without `else` cannot be used as a value; add an `else` branch
//    ╭─[headers_if_then_only_header.baml:8:13]
//  8 │     let y = if (false) { 1 };
//    ·             ────────────────
//    ╰────
// E0001
//
//   × `if` without `else` cannot be used as a value; add an `else` branch
//     ╭─[headers_if_then_only_header.baml:11:5]
//  11 │     y
//     ·     ─
//     ╰────
// E0001
//
//   × `if` without `else` cannot be used as a value; add an `else` branch
//     ╭─[headers_if_then_only_header.baml:22:13]
//  22 │ ╭─▶     let y = if (false) {
//  23 │ │           //## Then
//  24 │ │           1
//  25 │ ╰─▶     };
//     ╰────
// E0001
//
//   × `if` without `else` cannot be used as a value; add an `else` branch
//     ╭─[headers_if_then_only_header.baml:28:5]
//  28 │     y
//     ·     ─
//     ╰────
// E0001
//
//   × `if` without `else` cannot be used as a value; add an `else` branch
//     ╭─[headers_if_then_only_header.baml:40:13]
//  40 │ ╭─▶     let y = if (false) {
//  41 │ │           //## Then
//  42 │ │           1
//  43 │ ╰─▶     };
//     ╰────
// E0001
//
//   × `if` without `else` cannot be used as a value; add an `else` branch
//     ╭─[headers_if_then_only_header.baml:46:5]
//  46 │     y
//     ·     ─
//     ╰────
