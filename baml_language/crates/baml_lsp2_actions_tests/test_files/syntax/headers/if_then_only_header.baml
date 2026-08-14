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
// E0001
//
//   × cannot use return value of a void function
//    ╭─[headers_if_then_only_header.baml:8:13]
//  8 │     let y = if (false) { 1 };
//    ·             ────────────────
//    ╰────
// E0001
//
//   × mismatched types
//     ╭─[headers_if_then_only_header.baml:2:36]
//   2 │ ╭─▶ function IfThenOnlyHeader() -> int {
//   3 │ │       let x = 1;
//   4 │ │
//   5 │ │       //# Before
//   6 │ │
//   7 │ │       //# If
//   8 │ │       let y = if (false) { 1 };
//   9 │ │
//  10 │ │       //## After
//  11 │ │       y
//  12 │ ├─▶ }
//     · ╰──── expected `int`, found `void`
//     ╰────
// E0001
//
//   × cannot use return value of a void function
//     ╭─[headers_if_then_only_header.baml:22:13]
//  22 │ ╭─▶     let y = if (false) {
//  23 │ │           //## Then
//  24 │ │           1
//  25 │ ╰─▶     };
//     ╰────
// E0001
//
//   × mismatched types
//     ╭─[headers_if_then_only_header.baml:15:37]
//  15 │ ╭─▶ function IfThenOnlyHeader2() -> int {
//  16 │ │       let x = 1;
//  17 │ │       //## Before
//  18 │ │
//  19 │ │       let z = 5;
//  20 │ │
//  21 │ │       //### If
//  22 │ │       let y = if (false) {
//  23 │ │           //## Then
//  24 │ │           1
//  25 │ │       };
//  26 │ │
//  27 │ │       //## After
//  28 │ │       y
//  29 │ ├─▶ }
//     · ╰──── expected `int`, found `void`
//     ╰────
// E0001
//
//   × cannot use return value of a void function
//     ╭─[headers_if_then_only_header.baml:40:13]
//  40 │ ╭─▶     let y = if (false) {
//  41 │ │           //## Then
//  42 │ │           1
//  43 │ ╰─▶     };
//     ╰────
// E0001
//
//   × mismatched types
//     ╭─[headers_if_then_only_header.baml:33:37]
//  33 │ ╭─▶ function IfThenOnlyHeader2() -> int {
//  34 │ │       let x = 1;
//  35 │ │       //### Before
//  36 │ │
//  37 │ │       let z = 5;
//  38 │ │
//  39 │ │       //# If
//  40 │ │       let y = if (false) {
//  41 │ │           //## Then
//  42 │ │           1
//  43 │ │       };
//  44 │ │
//  45 │ │       //## After
//  46 │ │       y
//  47 │ ├─▶ }
//     · ╰──── expected `int`, found `void`
//     ╰────
