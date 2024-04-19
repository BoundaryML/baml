
function V2_FnEnumListOutput {
  (input: string  ) -> EnumOutput[]
}

function FnEnumListOutput_V2 {
  client GPT35
  prompt #"
    Print out two of these values randomly selected from the list below in a json array.

    {#print_enum(EnumOutput)}

    Answer:
  "#
}