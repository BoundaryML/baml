// The harness and its policy configure how one task is submitted. HarnessRun<T>
// retains the terminal T, events, portable history, and resume token.
class Harness<T> {
    harness: root.ai.Harness,
    options: root.ai.HarnessOptions,
    on_event: ((root.ai.AgentEvent) -> null throws never)?,

    function new(
        harness: root.ai.Harness,
        cwd: string? = null,
        permission_mode: string? = null,
        sandbox: string? = null,
        attributes: map<string, string> = {},
        on_event: ((root.ai.AgentEvent) -> null throws never)? = null,
    ) -> Harness<T> throws never {
        Harness<T> {
            harness: harness,
            options: root.ai.HarnessOptions {
                cwd: cwd,
                permission_mode: permission_mode,
                sandbox: sandbox,
                attributes: attributes,
            },
            on_event: on_event,
        }
    }

    implements root.ai.Runner<root.ai.Task<T>> {
        type Output = root.ai.HarnessRun<T>
        type Error = baml.errors.UnknownError

        function run(
            self,
            task: root.ai.Task<T>,
        ) -> root.ai.HarnessRun<T> throws baml.errors.UnknownError {
            //# Open an isolated harness session with workspace policy
            let session = self.harness.open(self.options);
            //# Run the harness while forwarding normalized events to this invocation
            self.harness.run<T>(session, task, on_event = self.on_event)
        }
    }
}
