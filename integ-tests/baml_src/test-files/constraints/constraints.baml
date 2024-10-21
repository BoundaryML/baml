// These classes and functions test several properties of
// constrains:
//
//  - The ability for constrains on fields to pass or fail.
//  - The ability for constraints on bare args and return types to pass or fail.
//  - The ability of constraints to influence which variant of a union is chosen
//    by the parser, when the structure is not sufficient to decide.


class Martian {
  age int @check({{ this < 30 }}, young_enough)
}

class Earthling {
  age int @check({{this < 200 and this > 0}}, earth_aged) @check({{this >1}}, no_infants)
}


class FooAny {
  planetary_age Martian | Earthling
  certainty int @check({{this == 102931}}, unreasonably_certain)
  species string @check({{this == "Homo sapiens"}}, trivial) @check({{this|regex_match("Homo")}}, regex_good) @check({{this|regex_match("neanderthalensis")}}, regex_bad)
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


function PredictAgeBare(inp: string @assert({{this|length > 1}}, big_enough)) -> int @check({{this == 10102}}, too_big) {
  client GPT35
  prompt #"
    Using your understanding of the historical popularity
    of names, predict the age of a person with the name
    {{ inp.name }} in years. Also predict their genus and
    species. It's Homo sapiens (with exactly that spelling).

    {{ctx.output_format}}
  "#
}

function ReturnFailingAssert(inp: int @assert({{this < 10}}, small_int)) -> int @assert({{this > 100}}, big_int) {
  client GPT35
  prompt #"
    Return the next integer after {{ inp }}.

    {{ctx.output_format}}
  "#
}

class TwoStoriesOneTitle {
  title string
  story_a string @assert( {{this|length > 1000000}}, too_long_story )
  story_b string @assert( {{this|length > 1000000}}, too_long_story )
}

function StreamFailingAssertion(theme: string, length: int) -> TwoStoriesOneTitle {
  client GPT35
  prompt #"
    Tell me two different stories along the theme of {{ theme }} with the same title.
    Please make each about {{ length }} words long.
    {{ctx.output_format}}
  "#
}
