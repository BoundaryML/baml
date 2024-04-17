function ExampleProvider {
  input string
  output LinterOutput[]
}

impl<llm, ExampleProvider> version1 {
  client GPT4Turbo
  prompt #"
    For the INSTRUCTIONS section, ensure that the "prompt" section contains an example of what the output json should look like given an example input and the type of the input. If the input is an object you can use a json representation of that example object. Be creative in the example you give. If the output is a string then there is no need to give an example.

   

    --------------------
    <INSTRUCTIONS>
    {#input}
    </INSTRUCTIONS>
    --------------------

    Output the example in this diagnostic format, where the suggestion will be a neatly written example with new lines and indentation like this:  "Here is an example:
    <EXAMPLE>
    INPUT
    ---
    {fill this out}
    ---
    OUTPUT JSON:
    {fill this out}
    ---", and the fix can just be null since we're not fixing anything. The exactPhrase should match the output format instructions.

    Return your answer in this JSON format:
    {#print_type(output)}

    Output JSON Array:
  "#
}
