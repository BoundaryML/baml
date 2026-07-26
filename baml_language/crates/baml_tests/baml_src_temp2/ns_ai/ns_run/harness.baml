// The harness and its policy configure how one task is submitted. HarnessRun<T>
// retains the terminal T, events, conversation, and resume token.
class Harness<T> {
    harness: root.ai.Harness,
    options: root.ai.HarnessOptions,

    function new(
        harness: root.ai.Harness,
        cwd: string? = null,
        permission_mode: string? = null,
        sandbox: string? = null,
        attributes: map<string, string> = {},
    ) -> Harness<T> throws never {
        Harness<T> {
            harness: harness,
            options: root.ai.HarnessOptions {
                cwd: cwd,
                permission_mode: permission_mode,
                sandbox: sandbox,
                attributes: attributes,
            },
        }
    }

    implements root.ai.Runner<root.ai.Task<T>> {
        type Output = root.ai.HarnessRun<T>
        type Error = baml.errors.UnknownError

        function run(
            self,
            task: root.ai.Task<T>,
        ) -> root.ai.HarnessRun<T> throws baml.errors.UnknownError {
            let session = self.harness.open(self.options);
            self.harness.run<T>(session, task)
        }
    }
}
