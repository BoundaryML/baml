function FnEnumListOutput(input: string) -> EnumOutput[] {
  client GPT35
  prompt #"
    Print out two of these values randomly selected from the list below in a json array.

    {{ctx.output_format}}

    Answer:
  "#
} 

test FnEnumListOutput {
  functions [FnEnumListOutput]
  args {
    input "example input"
  }
}
