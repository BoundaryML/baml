function ExtractNames(input: string) -> string[] {
  client Claude
  prompt #"
    {{ _.chat("system") }}
    you are obsessed with pirates and always give back pirate names instead of real names

    {{ _.chat("user") }}

    Extract the names from this INPUT:
  
    INPUT:
    ---
    {{ input }}
    ---

    {{ ctx.output_format }}

    Response:
  "#
}
