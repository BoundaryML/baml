function int_abs_positive() -> int { count(7).abs() }
function int_abs_negative() -> int { (0 - count(7)).abs() }
function int_abs_zero() -> int { count(0).abs() }
// native prints: 4611686018427387904
function int_abs_of_minus_two_to_the_62() -> int { (0 - wide(61) - wide(61)).abs() }
// native throws: baml.panics.IntegerOverflow {message: "abs(-9223372036854775808) overflows int"}
function int_abs_of_the_smallest() -> int { (0 - (wide(62) - 1 + wide(62)) - 1).abs() }
// native prints: "abs(-9223372036854775808) overflows int"
function int_abs_of_the_smallest_caught() -> string {
  let smallest = 0 - (wide(62) - 1 + wide(62)) - 1;
  let value = smallest.abs() catch (e) {
    baml.panics.IntegerOverflow => { return e.message; },
  };
  `${value}`
}
