
function FnEnumListOutput {
  input string
  output EnumOutput[]
}

impl<llm, FnEnumListOutput> v1 {
  client GPT35
  prompt #"
    Print out two of these values randomly selected from the list below in a json array.

    {#print_enum(EnumOutput)}

    Answer:
  "#
}