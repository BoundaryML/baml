function V2_FnOutputBool {
  (input: string  ) -> bool
}

function FnOutputBool_V2 {
  client GPT35
  prompt #"
    Return a {#print_type(output)}:  "#
}

