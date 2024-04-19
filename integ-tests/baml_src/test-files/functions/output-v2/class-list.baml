
function V2_FnOutputClassList {
  (input: string  ) -> TestOutputClass[]
}

function FnOutputClassList_V2 {
  client GPT35
  prompt #"
    Return a JSON array that follows this schema: 
    {#print_type(output)}
    JSON:
  "#
}
