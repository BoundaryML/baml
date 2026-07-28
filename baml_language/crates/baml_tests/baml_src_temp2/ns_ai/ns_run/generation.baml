// Thin runner values select which provider capability is used and therefore
// which output type Task.run returns.

class CompletionWithMeta<T> {
    function new() -> CompletionWithMeta<T> throws never {
        CompletionWithMeta<T> {  }
    }

    implements root.ai.Runner<root.ai.Task<T>> {
        type Output = root.ai.ResponseWithMetadata<T>
        type Error = root.ai.CallError | baml.errors.UnknownError

        function run(
            self,
            task: root.ai.Task<T>,
        ) -> root.ai.ResponseWithMetadata<T> throws root.ai.CallError | baml.errors.UnknownError {
            //# Select bounded completion and retain provider metadata
            match (task.provider) {
                let provider: root.ai.CompletionProvider => {
                    //## Complete the task once
                    provider.complete<T>(task)
                },
                _ => {
                    //## Reject providers without completion support
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
            //# Select bounded completion and project only the typed value
            match (task.provider) {
                let provider: root.ai.CompletionProvider => {
                    //## Complete the task once and discard response metadata
                    provider.complete<T>(task).value
                },
                _ => {
                    //## Reject providers without completion support
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
        type Output = root.ai.ResponseWithMetadata<T>
        type Error = root.ai.CallError | baml.errors.UnknownError

        function run(
            self,
            task: root.ai.Task<T>,
        ) -> root.ai.ResponseWithMetadata<T> throws root.ai.CallError | baml.errors.UnknownError {
            //# Select single-generation capability and retain provider metadata
            match (task.provider) {
                let provider: root.ai.GenerationProvider => {
                    //## Generate one typed response
                    provider.generate<T>(task)
                },
                _ => {
                    //## Reject providers without generation support
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
            //# Select single-generation capability and project only the typed value
            match (task.provider) {
                let provider: root.ai.GenerationProvider => {
                    //## Generate once and discard response metadata
                    provider.generate<T>(task).value
                },
                _ => {
                    //## Reject providers without generation support
                    throw baml.errors.Unsupported {
          message: "provider cannot perform one generation: " + task.provider_name(),
        }
                },
            }
        }
    }
}
