// Realtime remains unary because the Task carries the instructions, selected
// provider, and tools. The caller-owned Channel is configuration captured by
// the driver; Live owns the opened provider session.
class Live {
  channel: root.ai.Channel,
}

implements root.ai.Driver<root.ai.Task<null>> for Live {
  type Output = root.ai.Live
  type Error = baml.errors.UnknownError | baml.errors.Unsupported

  function drive(self, task: root.ai.Task<null>) -> root.ai.Live
      throws baml.errors.UnknownError | baml.errors.Unsupported {
    root.ai.drivers.open_live<null>(task, self.channel)
  }
}

function live(channel: root.ai.Channel) -> Live throws never {
  Live { channel: channel }
}
