
function FnUnionStringBoolWithArrayOutput {
  input string
  output float
}

impl<llm, FnUnionStringBoolWithArrayOutput> v1 {
  client GPT35
  prompt #"
    Return a random json blob:

    Answer in JSON format only.

    Answer:
  "#
}