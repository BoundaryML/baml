// The harness and its policy configure how one task is submitted. HarnessRun<T>
// retains the terminal T, events, portable history, and resume token.
class Harness<T> {
    harness: ai.harness.Harness,
    options: ai.harness.HarnessOptions,
    on_event: ((ai.observe.AgentEvent) -> null throws never)?,

    function new(
        harness: ai.harness.Harness,
        cwd: string? = null,
        permission_mode: string? = null,
        sandbox: string? = null,
        attributes: map<string, string> = {},
        on_event: ((ai.observe.AgentEvent) -> null throws never)? = null,
    ) -> Harness<T> throws never {
        Harness<T> {
            harness: harness,
            options: ai.harness.HarnessOptions {
                cwd: cwd,
                permission_mode: permission_mode,
                sandbox: sandbox,
                attributes: attributes,
            },
            on_event: on_event,
        }
    }

    implements ai.Runner<ai.Task<T>> {
        type Output = ai.harness.HarnessRun<T>
        type Error = ai.Failure | baml.errors.UnknownError

        function run(
            self,
            task: ai.Task<T>,
        ) -> ai.harness.HarnessRun<T> throws ai.Failure | baml.errors.UnknownError {
            //# Open an isolated harness session with workspace policy
            let session = self.harness.open(self.options);
            //# Run the harness while forwarding normalized events to this invocation
            self.harness.run<T>(session, task, on_event = self.on_event)
        }
    }
}
