function FnOutputStringList {
  input string  output string[]
}

impl<llm, FnOutputStringList> v1 {
  client GPT35
  prompt #"
    Return a list of strings:
  "#
} 