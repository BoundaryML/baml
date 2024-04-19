enum EnumOutput {
  ONE
  TWO
  THREE

  @@alias("VALUE_ENUM")
}

<<<<<<< HEAD
function V2_FnEnumOutput(param: string) -> EnumOutput {
||||||| parent of f21e6203 (finish)
function V2_FnEnumOutput {
  (input: string  ) -> EnumOutput
}

function FnEnumOutput_V2 {
=======
function V2_FnEnumOutput(input: string  ) -> EnumOutput {
>>>>>>> f21e6203 (finish)
  client GPT35
  prompt #"
    Choose one of these values randomly. Before you give the answer, write out an unrelated haiku about the ocean.

    {{ ctx.output_schema(prefix=null) }}
  "#
}
