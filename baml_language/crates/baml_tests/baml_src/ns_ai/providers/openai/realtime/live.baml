class CompletedLive {
  transcript: string,
  channel: Channel,

  implements Live {
    function events(self) -> LiveEventStream throws never {
      [TranscriptDelta { text: self.transcript }, LiveClosed {}]
    }
    function cancel_response(self) -> null throws never { null }
    function truncate_assistant_audio(self, played_ms: int) -> null throws never { null }
    function close(self) -> null throws never { self.channel.close() }
    function cleanup(self) -> null throws never { self.channel.close() }
  }
}
