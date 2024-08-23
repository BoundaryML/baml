function ExtractNames(input: string) -> string[] {
  client GPT4
  prompt #"
    Extract the names from this INPUT:
  
    INPUT:
    ---
    {{ input }}
    ---

    {{ ctx.output_format }}

    Response:
  "#
}
