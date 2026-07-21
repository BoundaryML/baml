// Bounded, process-local audio input. A realtime provider uses Channel instead;
// this value is for finite audio that can be part of one task.
class AudioStream {
  chunks: audio[],
  sample_rate_hz: int,
  channels: int,

  function collect(self) -> audio[] throws never { self.chunks }
}

