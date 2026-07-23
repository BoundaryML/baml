// Thin runner values select which provider capability is used and therefore
// which output type Task.run returns.

class CompletionWithMeta<T> {
  function new() -> CompletionWithMeta<T> throws never { CompletionWithMeta<T> {} }

  implements root.ai.Runner<root.ai.Task<T>> {
    type Output = root.ai.Response<T>
    type Error = baml.errors.CallError | baml.errors.UnknownError

    function run(self, task: root.ai.Task<T>) -> root.ai.Response<T>
        throws baml.errors.CallError | baml.errors.UnknownError {
      root.ai.drivers.complete_with_meta<T>(task)
    }
  }
}

class Completion<T> {
  function new() -> Completion<T> throws never { Completion<T> {} }

  implements root.ai.Runner<root.ai.Task<T>> {
    type Output = T
    type Error = baml.errors.CallError | baml.errors.UnknownError

    function run(self, task: root.ai.Task<T>) -> T
        throws baml.errors.CallError | baml.errors.UnknownError {
      root.ai.drivers.complete<T>(task)
    }
  }
}

class GenerationWithMeta<T> {
  function new() -> GenerationWithMeta<T> throws never { GenerationWithMeta<T> {} }

  implements root.ai.Runner<root.ai.Task<T>> {
    type Output = root.ai.Response<T>
    type Error = baml.errors.CallError | baml.errors.UnknownError

    function run(self, task: root.ai.Task<T>) -> root.ai.Response<T>
        throws baml.errors.CallError | baml.errors.UnknownError {
      root.ai.drivers.generate_with_meta<T>(task)
    }
  }
}

class Generation<T> {
  function new() -> Generation<T> throws never { Generation<T> {} }

  implements root.ai.Runner<root.ai.Task<T>> {
    type Output = T
    type Error = baml.errors.CallError | baml.errors.UnknownError

    function run(self, task: root.ai.Task<T>) -> T
        throws baml.errors.CallError | baml.errors.UnknownError {
      root.ai.drivers.generate<T>(task)
    }
  }
}
