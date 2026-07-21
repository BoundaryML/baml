// Interface-driven façade over the executable provider operations. The
// existing `ai.drivers.*` functions remain underneath during this comparison
// so the experiment changes the public composition model, not provider wire
// behavior.

class DriveWithMeta<T> {}

implements<T> root.ai.Driver<root.ai.Task<T>> for DriveWithMeta<T> {
  type Output = root.ai.Response<T>
  type Error = baml.errors.CallError | baml.errors.UnknownError

  function drive(self, task: root.ai.Task<T>) -> root.ai.Response<T>
      throws baml.errors.CallError | baml.errors.UnknownError {
    root.ai.drivers.drive_with_meta<T>(task)
  }
}

function drive_with_meta<T>() -> DriveWithMeta<T> throws never {
  DriveWithMeta<T> {}
}

class Drive<T> {}

implements<T> root.ai.Driver<root.ai.Task<T>> for Drive<T> {
  type Output = T
  type Error = baml.errors.CallError | baml.errors.UnknownError

  function drive(self, task: root.ai.Task<T>) -> T
      throws baml.errors.CallError | baml.errors.UnknownError {
    root.ai.drivers.drive<T>(task)
  }
}

function drive<T>() -> Drive<T> throws never {
  Drive<T> {}
}

class GenerateWithMeta<T> {}

implements<T> root.ai.Driver<root.ai.Task<T>> for GenerateWithMeta<T> {
  type Output = root.ai.Response<T>
  type Error = baml.errors.CallError | baml.errors.UnknownError

  function drive(self, task: root.ai.Task<T>) -> root.ai.Response<T>
      throws baml.errors.CallError | baml.errors.UnknownError {
    root.ai.drivers.generate_with_meta<T>(task)
  }
}

function generate_with_meta<T>() -> GenerateWithMeta<T> throws never {
  GenerateWithMeta<T> {}
}

class Generate<T> {}

implements<T> root.ai.Driver<root.ai.Task<T>> for Generate<T> {
  type Output = T
  type Error = baml.errors.CallError | baml.errors.UnknownError

  function drive(self, task: root.ai.Task<T>) -> T
      throws baml.errors.CallError | baml.errors.UnknownError {
    root.ai.drivers.generate<T>(task)
  }
}

function generate<T>() -> Generate<T> throws never {
  Generate<T> {}
}
