function TestOllama(input: string) -> string? {
  client Ollama
  prompt #"
    Write a nice haiku about {{ input }}
  "#
}

class Haiku {
  line1 string
  line2 string
  line3 string
}

function TestOllamaHaiku(input: string) -> Haiku {
  client Ollama
  prompt #"
    Write a nice haiku about {{ input }}

    {{ ctx.output_format }}
  "#
}
