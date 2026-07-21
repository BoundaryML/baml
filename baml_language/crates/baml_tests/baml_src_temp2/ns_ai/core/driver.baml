// Experimental nominal execution protocol for the BEPv2 interface-driven
// comparison package. `Input` is part of the interface head so each concrete
// driver can project an exact associated Output and Error type.
interface Driver<Input> {
  type Output
  type Error

  function drive(self, input: Input) -> Self.Output throws Self.Error
}
