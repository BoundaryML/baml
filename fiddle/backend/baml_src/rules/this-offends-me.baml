function OffensiveLanguage {
  input string
  output LinterOutput[]
}

impl<llm, OffensiveLanguage> version1 {
  client GPT4Turbo
  prompt #"
    Given the INSTRUCTIONS below, find ways in which the text could be considered offensive or inappropriate. If the text is offensive or inappropriate, return a diagnostic with the information on the offensiveness.

    --------------------
    <INSTRUCTIONS>
    {#input}
    </INSTRUCTIONS>
    --------------------


    Output the diagnostic in this JSON format (only include these fields, and no others):
    {#print_type(output)}

    Output JSON Array:
  "#
}
