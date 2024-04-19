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


function OptionalTest_Function_V2(input: string  ) -> (OptionalTest_ReturnType?)[] {
  client GPT35
  prompt #"
    

    Return a JSON blob with this schema: 
    {#print_type(output)}
    Here's a list of values you can use for
    {#print_enum(OptionalTest_CategoryType)}

    JSON:
  "#
}

