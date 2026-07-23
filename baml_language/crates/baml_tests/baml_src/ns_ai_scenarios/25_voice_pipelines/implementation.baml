// 25 — speech pipelines compose shared audio capabilities around an LLM task.

class s25_FakeStt {
  transcript: string,
  implements root.ai.SpeechToText { function transcribe(self, audio: string) -> string { self.transcript } }
}

class s25_FakeTts {
  implements root.ai.TextToSpeech { function synthesize(self, text: string) -> string { `audio<${text}>` } }
}
