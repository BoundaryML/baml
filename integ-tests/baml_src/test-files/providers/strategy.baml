function TestFallbackStrategy(input: string) -> string {
  client Resilient_SimpleSyntax
  prompt #"
    {{ _.role('system') }}
    You are a helpful assistant.

    {{ _.role('user') }}
    Write a nice short story about {{ input }}
  "#
}

function TestRoundRobinStrategy(input: string) -> string {
  client Lottery_SimpleSyntax
  prompt #"
    {{ _.role('system') }}
    You are a helpful assistant.

    {{ _.role('user') }}
    Write a nice short story about {{ input }}
  "#
}
