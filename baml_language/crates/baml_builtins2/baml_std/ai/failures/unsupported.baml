// The stdlib's Unsupported error rides the Failure channel: capability
// mismatches (a retry wrapper around a provider that cannot generate, a
// fallback chain with no capable member) are classified, terminal failures.
implements Failure for baml.errors.Unsupported {
  function effects(self) -> Effects throws never { Effects.None }
}
