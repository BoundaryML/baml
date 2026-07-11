interface ForkableSession requires Session {
  function fork(self) -> ForkableSession throws baml.errors.UnknownError
}
