
class BigNumbers {
  a int
  b float
}

class CompoundBigNumbers {
  big BigNumbers
  big_nums BigNumbers[]
  another BigNumbers
}

function StreamBigNumbers(digits: int) -> BigNumbers {
  client GPT35
  prompt #"
    Please make sure every integer in the output has {{ digits }} digits.
    For floats, provide a mix - from 0-10 places before the decimal point,
    and 1-10 places after the decimal point. Bet.

    {{ctx.output_format}}
  "#
}

function StreamOneBigNumber(digits: int) -> int {
  client GPT4
  prompt #"
    Respond with only an integer, no affirmations or prefixes or anything.
    The response should be parsable as a JSON number.

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

function StreamingCompoundNumbers(digits: int, yapping: bool) -> CompoundBigNumbers {
  client GPT4
  prompt #"

    {% if yapping %}
        Please give me a friendly response before outputting json. And put the JSON
        into a fenced code block.
    {% else %}
        Respond in pure json. Don't use any English descriptions like "Sure, I'll do that",
        nor put the result into a fenced code block.

        Just output a JSON value that could be parsed as JSON.
    {% endif %}

    Please make sure every integer has {{ digits }} digits.
    For floats, provide a mix - from 0-10 places before the decimal point,
    and 1-10 places after the decimal point. Bet.

    {{ctx.output_format}}
  "#
}
