// `bigint`: constants, the operators with an `int` on either side, values
// either side of what an `int` holds, conversions, and what fails.

// `n` as a `bigint`, built at run time.
function big(n: int) -> bigint {
  let x = 0n;
  for (let i = 0; i < n; i += 1) { x = x + 1n; };
  x
}
// Two to the `n`.
function big_two_to(n: int) -> bigint {
  let x = 1n;
  for (let i = 0; i < n; i += 1) { x = x * 2n; };
  x
}
class BigHolder { n: bigint, either: int | bigint }

function bigint_constant() -> bigint { 123456789012345678901234567890n }
function bigint_negative_constant() -> bigint { -98765432109876543210n }
function bigint_zero() -> bigint { big(0) }
function bigint_text() -> string {
  let x = big(5) - 7n;
  `${x} ` + string.from(x) + " " + x.to_string() + ` ${big_two_to(70)}`
}

// Either side of two to the 63rd and the 64th, where the limbs change.
function bigint_around_word() -> string {
  let p63 = big_two_to(63);
  let p64 = big_two_to(64);
  `${p63 - 1n} ${p63} ${p63 + 1n} ${0n - p63} ${0n - p63 - 1n} ${p64 - 1n} ${p64} ${0n - p64} ${0n - p64 - 1n}`
}
function bigint_from_int() -> string {
  let most = wide(61) + (wide(61) - 1);
  let least = 0 - most - 1;
  `${0n + most} ${0n + least} ${(0n + most) + 1} ${(0n + least) - 1} ${(0n + most) * 4 + 3}`
}
function bigint_shrinks() -> string {
  let p = big_two_to(130);
  `${p - p} ${(p + 5n) - p} ${p / p} ${(0n - p) / p} ${p - (p + 1n)}`
}

function bigint_add_sub() -> string {
  let a = big_two_to(80) + 12345n;
  let b = big(77);
  `${a + b} ${a - b} ${b - a} ${a + 5} ${5 + a} ${a - 5} ${5 - a} ${b + count(3)} ${count(3) - b}`
}
function bigint_mul() -> string {
  let a = big_two_to(70) + 1n;
  let b = 0n - big(1000003);
  `${a * b} ${b * a} ${a * 7} ${7 * a} ${a * 0} ${b * b} ${a * a * a}`
}
// Division truncates toward zero and the remainder has the dividend's sign.
function bigint_div_rem() -> string {
  let a = big_two_to(70) + 13n;
  let b = big(1000);
  let n = 0n - a;
  `${a / b} ${a % b} ${n / b} ${n % b} ${a / (0n - b)} ${a % (0n - b)} ${n / (0n - b)} ${n % (0n - b)} ${a / 7} ${a % 7} ${count(100) / b} ${count(100) % b} ${count(100000) / b} ${n % 7}`
}
function bigint_bitwise() -> string {
  let a = big_two_to(70) + 255n;
  let b = big(4080);
  let n = 0n - big(2);
  `${a & b} ${a | b} ${a ^ b} ${a & n} ${a | n} ${a ^ n} ${n & n} ${a & 15} ${15 | a} ${a ^ count(1)} ${(0 - count(1)) & a}`
}
function bigint_shifts() -> string {
  let a = big(5);
  let n = 0n - big(5);
  `${a << 70} ${a << big(70)} ${n << 3} ${big_two_to(90) >> 80} ${n >> 1} ${n >> 200} ${a >> 200} ${a >> big_two_to(70)} ${n >> big_two_to(70)} ${count(3) << big(64)} ${(0 - count(16)) >> big(2)} ${a << 0}`
}
function bigint_compare() -> string {
  let a = big_two_to(70);
  let b = a + 1n;
  let n = 0n - a;
  let small = big(5);
  `${a < b} ${a <= b} ${a > b} ${a >= b} ${a == b} ${a != b} ${a == b - 1n} ${n < a} ${n < small} ${small < a} ${a > small} ${small == big(5)} ${small != big(5)} ${big(4) < small} ${n >= big(0)} ${big_two_to(63) > big_two_to(62)} ${n <= n} ${(0n - b) < n}`
}
function bigint_negate() -> string {
  let a = big_two_to(63);
  `${-a} ${-(-a)} ${-big(0)} ${-(0n - big(7))}`
}

function bigint_abs() -> string {
  let p = big_two_to(63);
  `${(0n - p).abs()} ${p.abs()} ${big(0).abs()} ${(0n - big(3)).abs()}`
}
function bigint_isqrt() -> string {
  `${big_two_to(100).isqrt()} ${big(99).isqrt()} ${big(0).isqrt()} ${(big_two_to(101) - 1n).isqrt()}`
}
function bigint_isqrt_negative() -> bigint throws baml.errors.InvalidArgument { (0n - big(4)).isqrt() }
function bigint_pow() -> string {
  let three = big(3);
  `${three.pow(big(50))} ${three.pow(0n - big(1))} ${big(0).pow(big(0))} ${big(0).pow(big(9))} ${(0n - big(1)).pow(big_two_to(40))} ${(0n - big(1)).pow(big_two_to(40) + 1n)} ${(0n - three).pow(big(3))} ${big(1).pow(big_two_to(90))}`
}
function bigint_pow_too_large() -> string {
  let message = big(3).pow(big_two_to(40)) catch (e) {
    baml.panics.AllocFailure => e.message,
  };
  let wide = big(3).pow((big(4) * 100000000n)) catch (e) {
    baml.panics.AllocFailure => e.message,
  };
  `${message}; ${wide}`
}
function bigint_ilog() -> string {
  let p = big_two_to(100);
  `${p.ilog(big(2))} ${(p - 1n).ilog(big(2))} ${p.ilog(big(10))} ${big(1).ilog(big(7))} ${big(999).ilog(big(10))} ${big(1000).ilog(big(10))} ${p.ilog(p)} ${p.ilog(p + 1n)}`
}
function bigint_ilog_of_zero() -> bigint throws baml.errors.InvalidArgument { big(0).ilog(big(2)) }
function bigint_ilog_base_one() -> bigint throws baml.errors.InvalidArgument { big(8).ilog(big(1)) }
function bigint_to_int() -> string throws baml.errors.InvalidArgument {
  let most = big_two_to(62) - 1n;
  `${most.to_int()} ${(0n - most - 1n).to_int()} ${big(0).to_int()} ${(0n - big(42)).to_int()}`
}
function bigint_to_int_too_large() -> int throws baml.errors.InvalidArgument { big_two_to(62).to_int() }
function bigint_to_int_too_small() -> string {
  (0n - big_two_to(62) - 1n).to_int() catch (e) {
    baml.errors.InvalidArgument => e.message,
  };
  let wide = big_two_to(200).to_int() catch (e) {
    baml.errors.InvalidArgument => 0 - e.message.length(),
  };
  `${wide}`
}
function bigint_parse() -> string throws baml.errors.ParseError {
  let digits = rep("9", 30);
  `${bigint.parse(digits)} ${bigint.parse("-" + digits)} ${bigint.parse("+" + rep("0", 3) + "7")} ${bigint.parse(rep("0", 1))} ${bigint.parse("-0")}`
}
function bigint_parse_empty() -> bigint throws baml.errors.ParseError { bigint.parse(rep("", 1)) }
function bigint_parse_errors() -> string {
  let out = "";
  for (let text in ["12a", " 1", "1 ", "--1", "+", "1_000", "0x10", "1.0"]) {
    let message = string.from(bigint.parse(rep(text, 1))) catch (e) {
      baml.errors.ParseError => e.message,
    };
    out = out + message + ";";
  }
  out
}

function bigint_division_by_zero() -> bigint { big_two_to(70) / big(0) }
function bigint_remainder_by_zero_int() -> bigint { big(7) % count(0) }
function bigint_int_divided_by_zero() -> bigint { count(7) / big(0) }
function bigint_division_by_zero_caught() -> string {
  let wide = (big_two_to(70) % big(0)) catch (e) {
    baml.panics.DivisionByZero => match (e.dividend) {
      let value: bigint => value + 1n,
      let value: int => 0n - 1n,
    },
  };
  let narrow = (count(9) / big(0)) catch (e) {
    baml.panics.DivisionByZero => match (e.dividend) {
      let value: bigint => 0n - 1n,
      let value: int => 0n + value,
    },
  };
  `${wide} ${narrow}`
}
function bigint_negative_shift() -> bigint { big(1) << (0 - count(3)) }
function bigint_negative_shift_caught() -> string {
  let a = (big(1) << (0n - big(2))) catch (e) { baml.panics.NegativeBitShift => 0n - big(e.message.length()) };
  let b = (big(1) >> (0 - count(7))) catch (e) { baml.panics.NegativeBitShift => 0n - big(e.message.length()) };
  let c = (count(1) >> (0n - big_two_to(70))) catch (e) { baml.panics.NegativeBitShift => 0n - big(e.message.length()) };
  `${a} ${b} ${c}`
}
function bigint_too_large() -> string {
  let shifted = string.from(big(1) << (count(3) * 100000000)) catch (e) {
    baml.panics.AllocFailure => e.message,
  };
  let count_too_large = string.from(big(1) << big_two_to(70)) catch (e) {
    baml.panics.AllocFailure => e.message,
  };
  let p = big_two_to(27);
  let wide = big(1) << (count(2) * 100000000);
  let product = (wide * wide == wide) catch (e) {
    baml.panics.AllocFailure => e.message.length() > 0,
  };
  `${shifted}; ${count_too_large}; ${product}; ${p}`
}

function big_either(i: int) -> int | bigint {
  if (i % 2 == 0) { return count(i); }
  big_two_to(i)
}
function bigint_or_int() -> string {
  let out = "";
  for (let i = 0; i < 6; i += 1) {
    let text = match (big_either(i * 13)) {
      let value: bigint => `big ${value + 1}`,
      let value: int => `int ${value + 1}`,
    };
    out = out + text + ",";
  }
  out
}
function bigint_in_class_fields() -> string {
  let holders: BigHolder[] = [];
  for (let i = 0; i < 4; i += 1) {
    holders.push(BigHolder { n: big_two_to(60 + i * 3), either: big_either(i + 62) });
  }
  let total = 0n;
  for (let h in holders) {
    total = total + h.n + match (h.either) {
      let value: bigint => value,
      let value: int => 0n + value,
    };
  }
  `${total}`
}
function bigint_array() -> string {
  let held: bigint[] = [];
  for (let i = 0; i < 5; i += 1) { held.push(big_two_to(i * 33) - big(i)); }
  let out = `${held.length()}`;
  for (let x in held) { out = out + ` ${x}`; }
  out
}
function bigint_union_text() -> string {
  let out = "";
  for (let i = 70; i < 72; i += 1) {
    out = out + match (big_either(i)) {
      let value: bigint => `${value}n`,
      let value: int => `${value}`,
    } + " ";
  }
  out
}
function bigint_thrown() -> int throws BigHolder { throw BigHolder { n: 0n - big_two_to(64), either: count(6) } }
function bigint_factorial() -> bigint {
  let f = 1n;
  for (let i = 1; i <= 60; i += 1) { f = f * i; }
  f
}
function bigint_survives_collections() -> string {
  let kept: bigint[] = [];
  let sum = 0n;
  for (let i = 0; i < 200; i += 1) {
    let x = big_two_to(i % 97) + big(i);
    if (i % 7 == 0) { kept.push(x); }
    sum = sum + x;
  }
  for (let k in kept) { sum = sum - k; }
  `${sum} ${kept.length()}`
}
function bigint_duration() -> string {
  let d = baml.time.Duration.from_milliseconds(count(1500));
  let e = baml.time.Duration.from_seconds(big(2));
  `${d.to_nanoseconds()} ${d.to_microseconds()} ${d.to_milliseconds()} ${d.to_seconds()} ${e.to_nanoseconds()} ${e.to_minutes()}`
}

// The two halves of `bigint.random`: how many bytes a draw takes, and where
// its bytes land in the range.
function big_draw(n: int, byte: int) -> uint8array {
  let out = b"";
  for (let i = 0; i < n; i += 1) { out.push(byte); };
  out
}
function bigint_random_byte_count() -> string {
  let one = big(1);
  `${bigint._random_byte_count(one, one)} ${bigint._random_byte_count(one, big(2))} ${bigint._random_byte_count(big(0), big_two_to(8))} ${bigint._random_byte_count(big(0), big_two_to(8) + one)} ${bigint._random_byte_count(big(0) - big_two_to(200), big_two_to(200))}`
}
function bigint_random_in_range() -> bigint {
  bigint._random_in_range(big_draw(3, 5), big_two_to(4), big_two_to(20))
}
// A draw at or past the range's size is rejected: the answer is `upper`.
function bigint_random_in_range_rejected() -> bigint {
  bigint._random_in_range(big_draw(1, 255), big(1), big_two_to(7) + big(2))
}
// The bits past the range's are not part of the draw.
function bigint_random_in_range_masked() -> bigint {
  bigint._random_in_range(big_draw(2, 255), big(0), big_two_to(9))
}
function bigint_random_in_range_short_draw() -> bigint {
  bigint._random_in_range(big_draw(1, 1), big(1), big_two_to(30))
}
function bigint_random_in_range_empty() -> bigint {
  bigint._random_in_range(big_draw(4, 1), big_two_to(9), big_two_to(3))
}
function bigint_random_in_range_wide() -> bigint {
  bigint._random_in_range(big_draw(80, 3), big(3) - big_two_to(300), big_two_to(300))
}
