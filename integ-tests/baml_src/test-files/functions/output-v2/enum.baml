enum EnumOutput2 {
  ONE
  TWO
  THREE

  @@alias("VALUE_ENUM")
}

function V2_FnEnumOutput(input: string  ) -> EnumOutput2 {
  client GPT35
  prompt #"
    Choose one of these values randomly. Before you give the answer, write out an unrelated haiku about the ocean.

    {{ ctx.output_schema(prefix=null) }}
  "#
}
