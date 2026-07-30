// The byte-level format exchanged by a live session and an audio device.
// Concrete providers choose the encoding they support; the portable runner
// only verifies that both ends agree.
class RealtimeAudioFormat {
    encoding: string,
    sample_rate_hz: int,
    channels: int,

    function is_compatible_with(self, other: RealtimeAudioFormat) -> bool throws never {
        self.encoding == other.encoding
            && self.sample_rate_hz == other.sample_rate_hz
            && self.channels == other.channels
    }

    function describe(self) -> string throws never {
        `${self.encoding}, ${self.sample_rate_hz} Hz, ${self.channels} channel(s)`
    }
}

interface RealtimeAudioDevice requires ai.Resource {
  function input_format(self) -> RealtimeAudioFormat throws never
  function output_format(self) -> RealtimeAudioFormat throws never
  function next_input(self) -> uint8array? throws baml.errors.UnknownError
  function play_output(self, audio: uint8array) -> null
    throws baml.errors.UnknownError
  // The current assistant response has finished producing audio. A device may
  // now flush or begin playing the chunks queued by play_output().
  function finish_output(self) -> null throws baml.errors.UnknownError
  function stop_output(self) -> int throws baml.errors.UnknownError
  function close(self) -> null throws never
}
