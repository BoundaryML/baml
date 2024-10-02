// These classes and functions test several properties of
// constrains:
//
//  - The ability for constrains on fields to pass or fail.
//  - The ability for constraints on bare args and return types to pass or fail.
//  - The ability of constraints to influence which variant of a union is chosen
//    by the parser, when the structure is not sufficient to decide.


class Martian {
  age int @check({{ this < 30 }}, "young_enough")
}

class Earthling {
  age int @check({{this < 200 and this > 0}}, "earth_aged") @check({{this >1}}, "no_infants")
}


class FooAny {
  planetary_age Martian | Earthling
  certainty int @check({{this == 102931}}, "unreasonably_certain")
  species string @check({{this == "Homo sapiens"}}, "trivial") @check({{this|regex_match("Homo")}}, "regex_good") @check({{this|regex_match("neanderthalensis")}}, "regex_bad")
}


class InputWithConstraint {
  name string @assert({{this|length > 1}}, "nonempty")
  amount int @check({{ this < 1 }}, "small")
}

function PredictAge(name: string) -> FooAny {
  client GPT35
  prompt #"
    Using your understanding of the historical popularity
    of names, predict the age of a person with the name
    {{ name }} in years. Also predict their genus and
    species. It's Homo sapiens (with exactly that spelling
    and capitalization). I'll give you a hint: If the name
    is "Greg", his age is 41.

    {{ctx.output_format}}
  "#
}


function PredictAgeComplex(inp: InputWithConstraint) -> InputWithConstraint {
  client GPT35
  prompt #"
    Using your understanding of the historical popularity
    of names, predict the age of a person with the name
    {{ inp.name }} in years. Also predict their genus and
    species. It's Homo sapiens (with exactly that spelling).

    {{ctx.output_format}}
  "#
}

function PredictAgeBare(inp: string @assert({{this|length > 1}}, "big_enough")) -> int @check({{this == 10102}}, "too_big") {
  client GPT35
  prompt #"
    Using your understanding of the historical popularity
    of names, predict the age of a person with the name
    {{ inp.name }} in years. Also predict their genus and
    species. It's Homo sapiens (with exactly that spelling).

    {{ctx.output_format}}
  "#
}
