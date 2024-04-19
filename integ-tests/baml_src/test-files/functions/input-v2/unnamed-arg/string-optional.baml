function V2_FnStringOptional(input: string?) -> string {  // TODO: playground wont accept a null string input. Must fix
  client GPT35
  prompt #"
    Return {{input}}:  "#
}
