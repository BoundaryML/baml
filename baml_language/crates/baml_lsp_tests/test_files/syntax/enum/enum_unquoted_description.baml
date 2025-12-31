enum TestEnum {
  Angry @alias("k1") @description(#"
    User is angry
  "#)
  Happy @alias("k2") @description(#"
    User is happy
  "#)
  // tests whether k1 doesnt incorrectly get matched with k11
  Sad @alias("k3") @description(#"
    User is sad
  "#)
  Confused @description(
    User is confused
  )
  Excited @alias("k5") @description(
    User is excited
  )
  Exclamation @alias("k6") // only alias
  
  Bored @alias("k7") @description(#"
    User is bored
    With a long description
  "#)
   
  @@alias("Category")
}

//----
//- diagnostics
// Error: Expected RParen, found Word
//     ╭─[ 0:13:10 ]
//     │
//  13 │     User is confused
//     │          ─┬  
//     │           ╰── Expected RParen, found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected Unexpected token in enum body, found RParen
//     ╭─[ 0:14:3 ]
//     │
//  14 │   )
//     │   ┬  
//     │   ╰── Expected Unexpected token in enum body, found RParen
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected RParen, found Word
//     ╭─[ 0:16:10 ]
//     │
//  16 │     User is excited
//     │          ─┬  
//     │           ╰── Expected RParen, found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected Unexpected token in enum body, found RParen
//     ╭─[ 0:17:3 ]
//     │
//  17 │   )
//     │   ┬  
//     │   ╰── Expected Unexpected token in enum body, found RParen
//     │ 
//     │ Note: Error code: E0010
// ────╯
