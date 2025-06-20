/// An enum with three values,
/// ONE, TWO and THREE.
enum EnumOutput {

  /// The first enum.
  ONE

  /// The second enum.
  TWO @alias("two") @description("two")
  THREE @alias("hi") @description("three")

  @@alias("VALUE_ENUM")
}

function FnEnumOutput(input: string) -> EnumOutput {
  client GPT35
  prompt #"
    Choose one of these values randomly. Before you give the answer, write out an unrelated haiku about the ocean.

    {{ctx.output_format(prefix=null)}}
  "#
}

test FnEnumOutput {
  functions [FnEnumOutput]
  args {
    input "example input"
  }
}
