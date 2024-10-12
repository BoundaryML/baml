
class BigNumbers {
  a int
  b int
  c int
  d int
  e int
  f int
}

function StreamBigNumbers(digits: int) -> BigNumbers {
  client GPT35
  prompt #"
    Please make sure every integer in the output has {{ digits }} digits.

    {{ctx.output_format}}
  "#
}

function StreamOneBigNumber(digits: int) -> int {
  client GPT4
  prompt #"
    Please make sure the integer has {{ digits }} digits.

    {{ctx.output_format}}
  "#
}

function StreamUnionIntegers(digits: int) -> (int | string)[] {
  client GPT4
  prompt #"
    Please make sure the first item is an integer,
    and there are only 2 items in the list.
    Any integers must have {{ digits }} digits.

    Do not put quotes around the integer. If you chose to generate
    a string, please don't make it the string representation
    of an integer. Instead use common dog names.

    {{ctx.output_format}}
  "#
}
