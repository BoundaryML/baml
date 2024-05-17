
function V2_FnOutputClassList(input: string  ) -> TestOutputClass[] {
  client GPT35
  prompt #"
    Return a JSON array that follows this schema:
    {{ ctx.output_format(prefix=null) }}
    JSON:
  "#
}
