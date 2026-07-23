// Realtime remains unary because the Task carries the instructions, selected
// provider, and tools. The caller-owned Channel is configuration captured by
// the runner; Live owns the opened provider session.
class Realtime {
  channel: root.ai.Channel,

  function new(channel: root.ai.Channel) -> Realtime throws never {
    Realtime { channel: channel }
  }

  implements root.ai.Runner<root.ai.Task<null>> {
    type Output = root.ai.Live
    type Error = baml.errors.UnknownError | baml.errors.Unsupported

    function run(self, task: root.ai.Task<null>) -> root.ai.Live
        throws baml.errors.UnknownError | baml.errors.Unsupported {
      root.ai.drivers.open_live<null>(task, self.channel)
    }
  }
}
