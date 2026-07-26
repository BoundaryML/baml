// Thin runner values select which provider capability is used and therefore
// which output type Task.run returns.

class CompletionWithMeta<T> {
    function new() -> CompletionWithMeta<T> throws never {
        CompletionWithMeta<T> {  }
    }

    implements root.ai.Runner<root.ai.Task<T>> {
        type Output = root.ai.Response<T>
        type Error = root.ai.CallError | baml.errors.UnknownError

        function run(
            self,
            task: root.ai.Task<T>,
        ) -> root.ai.Response<T> throws root.ai.CallError | baml.errors.UnknownError {
            match (task.provider) {
                let provider: root.ai.CompletionProvider => provider.complete<T>(task),
                _ => {
                    throw baml.errors.Unsupported {
          message: "provider cannot complete a task: " + task.provider_name(),
        }
                },
            }
        }
    }
}

class Completion<T> {
    function new() -> Completion<T> throws never {
        Completion<T> {  }
    }

    implements root.ai.Runner<root.ai.Task<T>> {
        type Output = T
        type Error = root.ai.CallError | baml.errors.UnknownError

        function run(
            self,
            task: root.ai.Task<T>,
        ) -> T throws root.ai.CallError | baml.errors.UnknownError {
            match (task.provider) {
                let provider: root.ai.CompletionProvider => provider.complete<T>(task).value,
                _ => {
                    throw baml.errors.Unsupported {
          message: "provider cannot complete a task: " + task.provider_name(),
        }
                },
            }
        }
    }
}

class GenerationWithMeta<T> {
    function new() -> GenerationWithMeta<T> throws never {
        GenerationWithMeta<T> {  }
    }

    implements root.ai.Runner<root.ai.Task<T>> {
        type Output = root.ai.Response<T>
        type Error = root.ai.CallError | baml.errors.UnknownError

        function run(
            self,
            task: root.ai.Task<T>,
        ) -> root.ai.Response<T> throws root.ai.CallError | baml.errors.UnknownError {
            match (task.provider) {
                let provider: root.ai.GenerationProvider => provider.generate<T>(task),
                _ => {
                    throw baml.errors.Unsupported {
          message: "provider cannot perform one generation: " + task.provider_name(),
        }
                },
            }
        }
    }
}

class Generation<T> {
    function new() -> Generation<T> throws never {
        Generation<T> {  }
    }

    implements root.ai.Runner<root.ai.Task<T>> {
        type Output = T
        type Error = root.ai.CallError | baml.errors.UnknownError

        function run(
            self,
            task: root.ai.Task<T>,
        ) -> T throws root.ai.CallError | baml.errors.UnknownError {
            match (task.provider) {
                let provider: root.ai.GenerationProvider => provider.generate<T>(task).value,
                _ => {
                    throw baml.errors.Unsupported {
          message: "provider cannot perform one generation: " + task.provider_name(),
        }
                },
            }
        }
    }
}
