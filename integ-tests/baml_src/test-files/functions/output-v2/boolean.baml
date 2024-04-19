function V2_FnOutputBool(input: string  ) -> bool {
  client GPT35
  prompt #"
    Return a {{ ctx.output_format(prefix=null) }}:  "#
}

