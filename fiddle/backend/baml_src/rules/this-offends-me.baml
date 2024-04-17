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


function NoTipping {
  input string
  output LinterOutput[]
}

impl<llm, NoTipping> version1 {
  client GPT4Turbo
  prompt #"
    Given the INSTRUCTIONS below, check if the text asks for tips. If so, return a diagnostic with the information on the request.

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



function NoLargeDistance {
  input string
  output LinterOutput[]
}

impl<llm, NoLargeDistance> version1 {
  client GPT4Turbo
  prompt #"
    Given the INSTRUCTIONS below,
    check if any data model's fields have constraints that are placed too far apart in the text.

    Techniques like reasonging, summarization, and paraphrasing may be placed far apart in the text.

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
