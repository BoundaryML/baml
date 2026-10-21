class Input {
  text string
}

client<llm> GPT4 {
  provider openai
  options {
    model "gpt-4"
  }
}

function Summarize(input: Input) -> string {
  client GPT4
  prompt #"
    Summarize: {{ input.text }}
  "#
}

test "BasicTest" {
  Summarize(Input { text: "Hello world" })
}
