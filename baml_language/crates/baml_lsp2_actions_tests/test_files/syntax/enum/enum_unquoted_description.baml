enum TestEnum {
  Angry @alias("k1") @description("
    User is angry
  ")
  Happy @alias("k2") @description("
    User is happy
  ")
  // tests whether k1 doesnt incorrectly get matched with k11
  Sad @alias("k3") @description("
    User is sad
  ")
  Confused @description(
    User is confused
  )
  Excited @alias("k5") @description(
    User is excited
  )
  Exclamation @alias("k6") // only alias

  Bored @alias("k7") @description("
    User is bored
    With a long description
  ")

  @@alias("Category")
}

//----
//- diagnostics
// E0010
//
//   × unexpected token
//     ╭─[enum_enum_unquoted_description.baml:13:10]
//  13 │     User is confused
//     ·          ─┬
//     ·           ╰── expected `')'`, found `is`
//     ╰────
// E0010
//
//   × unexpected token
//     ╭─[enum_enum_unquoted_description.baml:14:3]
//  14 │   )
//     ·   ┬
//     ·   ╰── expected `Unexpected token in enum body`, found `')'`
//     ╰────
// E0010
//
//   × unexpected token
//     ╭─[enum_enum_unquoted_description.baml:16:10]
//  16 │     User is excited
//     ·          ─┬
//     ·           ╰── expected `')'`, found `is`
//     ╰────
// E0010
//
//   × unexpected token
//     ╭─[enum_enum_unquoted_description.baml:17:3]
//  17 │   )
//     ·   ┬
//     ·   ╰── expected `Unexpected token in enum body`, found `')'`
//     ╰────
// E0037
//
//   × `@description` argument must be a string literal, got `User`
//     ╭─[enum_enum_unquoted_description.baml:13:5]
//  13 │     User is confused
//     ·     ──┬─
//     ·       ╰── invalid builtin declaration
//     ╰────
// E0037
//
//   × `@description` argument must be a string literal, got `User`
//     ╭─[enum_enum_unquoted_description.baml:16:5]
//  16 │     User is excited
//     ·     ──┬─
//     ·       ╰── invalid builtin declaration
//     ╰────
