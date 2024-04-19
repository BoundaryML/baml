function V2_FnOutputBool(input: string  ) -> bool {
  client GPT35
  prompt #"
    Return a {#print_type(output)}:  "#
}

