// Provider- and harness-owned resources share one idempotent cleanup verb.
// Domain verbs such as cancel, close, and delete remain useful aliases.
interface Resource {
  function cleanup(self) -> null throws baml.errors.UnknownError
}
