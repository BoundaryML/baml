function V2_FnOutputStringList {
  (input: string  ) -> string[]
}

function FnOutputStringList_V2 {
  client GPT35
  prompt #"
    Return a list of strings:
  "#
} 