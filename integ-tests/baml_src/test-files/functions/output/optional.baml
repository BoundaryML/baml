class OptionalTest_Prop1 {
  omega_a string
  omega_b int
}

enum OptionalTest_CategoryType {
  Aleph
  Beta
  Gamma
}
 
class OptionalTest_ReturnType {
  omega_1 OptionalTest_Prop1?
  omega_2 string?
  omega_3 (OptionalTest_CategoryType?)[]
} 
 
function OptionalTest_Function(input: string) -> (OptionalTest_ReturnType?)[]
{
  client GPT35
  prompt #"
    Return a JSON blob with this schema: 
    {{ctx.output_format}}

    JSON:
  "#
}

test OptionalTest_Function {
  functions [OptionalTest_Function]
  args {
    input "example input"
  }
}
