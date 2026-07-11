interface SpeechToText {
  function transcribe(self, audio: string) -> string
}

interface TextToSpeech {
  function synthesize(self, text: string) -> string
}
