// Provider- and harness-owned resources expose domain lifecycle verbs such as
// cancel, close, and delete.
interface Resource {
  // The generic cleanup surface is intentionally disabled in these examples.
  // function cleanup(self) -> void throws never
}
