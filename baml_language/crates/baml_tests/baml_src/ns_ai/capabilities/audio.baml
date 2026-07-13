interface SpeechToText {
  function transcribe(self, audio: string) -> string throws never
}

interface TextToSpeech {
  function synthesize(self, text: string) -> string throws never
}
