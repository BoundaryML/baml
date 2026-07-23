// Provider- and harness-owned resources share one idempotent cleanup verb.
// Domain verbs such as cancel, close, and delete remain useful aliases.
interface Resource {
  // This exact direct-method shape is recognized by the VM as the BEP-042
  // at-most-once finalizer. Implementations must define it on the class body.
  function cleanup(self) -> void throws never
}
