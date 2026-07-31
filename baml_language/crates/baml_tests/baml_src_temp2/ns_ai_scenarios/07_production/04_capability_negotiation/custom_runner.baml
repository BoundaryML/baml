class NegotiatedRun {
    provider: string,
    mode: string,
    output: string,
}

// Custom runner that narrows an erased task provider to its strongest
// interaction capability, then executes the same Task through that capability.
class CapabilityNegotiationRunner {
    function new() -> CapabilityNegotiationRunner throws never {
        CapabilityNegotiationRunner {  }
    }

    implements ai.Runner<ai.Task<Resolution>> {
        type Output = NegotiatedRun
        type Error = ai.Failure
            | baml.errors.UnknownError
            | ai.Failure
            | baml.errors.Unsupported
            | baml.errors.Io
            | baml.errors.LlmClient

        function run(
            self,
            task: ai.Task<Resolution>,
        ) -> NegotiatedRun throws ai.Failure
            | baml.errors.UnknownError
            | ai.Failure
            | baml.errors.Unsupported
            | baml.errors.Io
            | baml.errors.LlmClient
{
            //# Narrow the erased provider to the strongest supported interaction
            match (task.provider) {
                let realtime: ai.realtime.RealtimeProvider => {
                    let live_session = realtime
                        .open_live<Resolution>(
                            task,
                            ai.testing.RecordingChannel { frames: [] },
                        );
                    NegotiatedRun {
                        provider: realtime.name(),
                        mode: "realtime",
                        output: ai.realtime.collect_live_text(live_session),
                    }
                },
                let stream: ai.StreamingProvider => {
                    let resolution = stream.stream<Resolution$stream, Resolution>(task).final();
                    NegotiatedRun {
                        provider: stream.name(),
                        mode: "stream",
                        output: resolution.reply,
                    }
                },
                let agent: ai.AgentProvider => {
                    let resolution = done_or_fail(
                            task.with_provider(agent)
                                .run(runner = ai.run.Agent<Resolution>.new()),
                        )
                        .value;
                    NegotiatedRun {
                        provider: agent.name(),
                        mode: "agent",
                        output: resolution.reply,
                    }
                },
                _ => {
                    throw baml.errors.Unsupported {
                        message: "task provider cannot resolve a ticket: " + task.provider_name(),
                    }
                },
            }
        }
    }
}
