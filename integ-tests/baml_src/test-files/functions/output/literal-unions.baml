function LiteralUnionsTest(input: string) -> 1 | true | "string output" {
  client GPT35
  prompt #"
    Return one of these values without any additional context: 
    {{ctx.output_format}}
  "#
}

// TODO: expr func
// function LiteralUnionsTestExprFunc(input: string) -> 1 | true | "string output" {
//   return 1;
// }

test LiteralUnionsTest {
  functions [LiteralUnionsTest]
  args {
    input "example input"
  }
}
