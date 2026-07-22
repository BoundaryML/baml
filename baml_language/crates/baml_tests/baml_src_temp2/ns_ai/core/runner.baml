// Nominal execution protocol for the BEPv2 runner-oriented comparison.
// `Input` is part of the interface head so each concrete runner projects an
// exact associated Output and Error type.
interface Runner<Input> {
  type Output
  type Error

  function run(self, input: Input) -> Self.Output throws Self.Error
}
