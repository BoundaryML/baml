// Realtime remains unary because the Task carries the instructions, selected
// provider, and tools. The caller-owned Channel is configuration captured by
// the runner; LiveSession owns the opened provider session.
class Realtime {
    channel: root.ai.Channel,
    auto_execute_tools: bool,

    function new(
        channel: root.ai.Channel,
        auto_execute_tools: bool = true,
    ) -> Realtime throws never {
        Realtime { channel: channel, auto_execute_tools: auto_execute_tools }
    }

    implements root.ai.Runner<root.ai.Task<null>> {
        type Output = root.ai.LiveSession
        type Error = baml.errors.UnknownError | baml.errors.Unsupported

        function run(
            self,
            task: root.ai.Task<null>,
        ) -> root.ai.LiveSession throws baml.errors.UnknownError | baml.errors.Unsupported {
            let live_session = match (task.provider) {
                let provider: root.ai.RealtimeProvider => {
                    provider.open_live<null>(task, self.channel)
                },
                _ => {
                    throw baml.errors.Unsupported {
                    message: "provider cannot open a realtime channel: " + task.provider_name(),
                }
                },
            };
            if (self.auto_execute_tools && task.tools.length() > 0) {
                root.ai.AutomaticToolLiveSession { inner: live_session, tools: task.tools }
            } else {
                live_session
            }
        }
    }
}
