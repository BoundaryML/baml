function TestAnthropic(input: string) -> string {
  client Claude
  prompt #"
    Write a nice haiku about {{ input }}
  "#
}

function TestAnthropicShorthand(input: string) -> string {
  client "anthropic/claude-3-haiku-20240307"
  prompt #"
    Write a nice short story about {{ input }}. Keep it to 15 words or less.
  "#
}

function TestCaching(input: string, not_cached: string) -> string {
  client ClaudeWithCaching
  prompt #"
    {{ _.role('system', cache_control={"type": "ephemeral"}) }}
    Generate the following story
    {{ input }}

    {# Haiku require 2048 tokens to cache -#}
    {{ input }}

    {{ _.role('user') }}
    {{ not_cached }}
  "#
}

class CustomStory {
  title string
  characters string[]
  content string
}

function TestThinking(input: string) -> CustomStory {
  client SonnetThinking
  prompt #"
    {{ _.role('system') }}
    Generate the following story
    {{ ctx.output_format }}

    {{ _.role('user') }}
    {{ input }}
  "#
}