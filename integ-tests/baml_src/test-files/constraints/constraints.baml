// These classes and functions test several properties of
// constrains:
//
//  - The ability for constrains on fields to pass or fail.
//  - The ability for constraints on bare args and return types to pass or fail.
//  - The ability of constraints to influence which variant of a union is chosen
//    by the parser, when the structure is not sufficient to decide.


class Martian {
  age int @check(young_enough, {{ this < 30 }})
}

class Earthling {
  age int @check(earth_aged, {{this < 200 and this > 0}}) @check(no_infants, {{this >1}})
}


class FooAny {
  planetary_age Martian | Earthling
  certainty int @check(unreasonably_certain, {{this == 102931}})
  species string @check(trivial, {{this == "Homo sapiens"}}) @check(regex_good, {{this|regex_match("Homo")}}) @check(regex_bad, {{this|regex_match("neanderthalensis")}})
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


function PredictAgeBare(inp: string @assert(big_enough, {{this|length > 1}})) -> int @check(too_big, {{this == 10102}}) {
  client GPT35
  prompt #"
    Using your understanding of the historical popularity
    of names, predict the age of a person with the name
    {{ inp.name }} in years. Also predict their genus and
    species. It's Homo sapiens (with exactly that spelling).

    {{ctx.output_format}}
  "#
}

function ReturnFailingAssert(inp: int @assert(small_int, {{this < 10}})) -> int @assert(big_int, {{this > 100}}) {
  client GPT35
  prompt #"
    Return the next integer after {{ inp }}.

    {{ctx.output_format}}
  "#
}

class TwoStoriesOneTitle {
  title string
  story_a string @assert(too_long_story, {{this|length > 1000000}} )
  story_b string @assert(too_long_story, {{this|length > 1000000}} )
}

function StreamFailingAssertion(theme: string, length: int) -> TwoStoriesOneTitle {
  client GPT35
  prompt #"
    Tell me two different stories along the theme of {{ theme }} with the same title.
    Please make each about {{ length }} words long.
    {{ctx.output_format}}
  "#
}
