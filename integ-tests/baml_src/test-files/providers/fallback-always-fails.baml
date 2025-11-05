client<llm> FallbackAlwaysFails {
  provider baml-fallback
  options {
    strategy [
        "openai/gpt-0-noexist"
        "openai/gpt-1-noexist"
        "openai/gpt-2-noexist"
    ]
  }
}

function FnAlwaysFails(input: string) -> string {
  client "openai/gpt-0-noexist"
  prompt #"
    {{ _.role('system') }}
    You are a helpful assistant.

    {{ _.role('user') }}
    Write a nice short story about {{ input }}
  "#
}

function FnFallbackAlwaysFails(input: string) -> string {
  client FallbackAlwaysFails
  prompt #"
    {{ _.role('system') }}
    You are a helpful assistant.

    {{ _.role('user') }}
    Write a nice short story about {{ input }}
  "#
}
