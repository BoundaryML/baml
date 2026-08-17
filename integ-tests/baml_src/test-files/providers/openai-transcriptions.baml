client<llm> OpenAIGPTTranscribe {
  provider openai-transcriptions
  options {
    api_key env.OPENAI_API_KEY
    model "gpt-transcribe"
  }
}

function TestOpenAITranscription(audio: audio) -> string {
  client OpenAIGPTTranscribe
  prompt #"
    {{ _.role("user") }}
    {{ audio }}
  "#
}

function TestOpenAITranscriptionMultipartChat(audio: audio) -> string {
  client OpenAIGPTTranscribe
  prompt #"
    {{ _.role("system") }}
    Preserve the wording and punctuation from the recording.

    {{ _.role("user") }}
    Transcribe this audio:
    {{ audio }}
    Return only the spoken words.
  "#
}
