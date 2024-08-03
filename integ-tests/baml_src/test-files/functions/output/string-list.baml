function FnOutputStringList(input: string) -> string[] {
  client GPT35
  prompt #"
    Return a list of strings in json format like ["string1", "string2", "string3"].

    JSON:
  "#
}

test FnOutputStringList {
  functions [FnOutputStringList]
  args {
    input "example input"
  }
}
