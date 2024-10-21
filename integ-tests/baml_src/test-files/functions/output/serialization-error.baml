class DummyOutput {
  nonce string
  nonce2 string
  @@dynamic
}

function DummyOutputFunction(input: string) -> DummyOutput {
  client GPT35
  prompt #"
    Say "hello there".
  "#
}