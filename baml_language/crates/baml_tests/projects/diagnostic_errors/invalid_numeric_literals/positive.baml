// Valid numeric literals in every base, with underscore separators; must
// compile without diagnostics.

function good_literals() -> int {
  let hex = 0xFF;
  let hex_lower = 0xcafe;
  let octal = 0o755;
  let binary = 0b1010;
  let plain_zero_prefixed = 0x0;
  let underscores = 1_000_000;
  let trailing_underscore = 1_;
  let hex_underscores = 0xFF_FF;
  let underscore_after_prefix = 0x_F;
  let int_max = 0x3FFF_FFFF_FFFF_FFFF;
  let negative = -0xFF;
  let big_hex = 0xFFFF_FFFF_FFFF_FFFF_FFFFn;
  let big_underscores = 1_000n;
  let float_mantissa = 1_000.5;
  let float_exponent = 1e1_0;
  hex + octal + binary
}

// Literal types accept the new bases.
type ReadFlag = 0x1
type Permissions = 0o644 | 0o755
type Mask = 0b1111 | 0b0000
