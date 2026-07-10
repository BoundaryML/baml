// 25 — speech pipelines are ordinary composition around an LLM task.
interface s25_Stt { function transcribe(self, audio: string) -> string }
interface s25_Tts { function synthesize(self, text: string) -> string }

class s25_FakeStt {
  transcript: string,
  implements s25_Stt { function transcribe(self, audio: string) -> string { self.transcript } }
}

class s25_FakeTts {
  implements s25_Tts { function synthesize(self, text: string) -> string { `audio<${text}>` } }
}
