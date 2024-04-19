class OptionalTest_Prop1v2 {
  omega_a string
  omega_b int
}

enum OptionalTest_CategoryTypev2 {
  Aleph
  Beta
  Gamma
}

class OptionalTest_ReturnTypev2 {
  omega_1 OptionalTest_Prop1v2?
  omega_2 string?
  omega_3 (OptionalTest_CategoryTypev2?)[]
}


function OptionalTest_Function_V2(input: string  ) -> (OptionalTest_ReturnTypev2?)[] {
  client GPT35
  prompt #"
    

    Return a JSON blob with this schema: 
    {#print_type(output)}
    Here's a list of values you can use for
    {#print_enum(OptionalTest_CategoryTypev2)}

    JSON:
  "#
}

