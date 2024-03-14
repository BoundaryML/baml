enum EnumOutput {
  ONE
  TWO
  THREE

  @@alias("VALUE_ENUM")
}

function FnEnumOutput {
  input string
  output EnumOutput
}

impl<llm, FnEnumOutput> v1 {
  client GPT35
  prompt #"
    Choose one of these values randomly. Before you give the answer, write out an unrelated haiku about the ocean.

    {#print_enum(EnumOutput)}
  "#
}