class Message {
  role "user" | "assistant" | string
  message string
}

function Bot(convo: Message[]) -> string {
  client "openai/gpt-4o"
  prompt #"
    You are a helpful assistant.
    {{ ctx.output_format }}

    {% for m in convo %}
    {{ _.role(m.role) }}
    {{ m.message }}
    {% endfor %}
  "#
}
