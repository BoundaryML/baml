enum Characteristic {
  Kind
  Mean
  Daring
  Cunning
  Noble
}

class Character {
  name string
  age int?
  characteristics Characteristic[]
  relationships map<string, string>
}

function ExtractCharacters(story: string) -> Character[] {
  client GPT4
  prompt #"
    Extract the Characters from the given story. Order them by importance:
    {{ story }}
    {{ ctx.output_format }}
  "#
}

test Extract {
  functions [ExtractCharacters]
  args {
    story #"
      Tyrion Lannister, the 30-year-old son of the noble Tywin,
      devised a plan to see which of his friends was loyal. He told
      Varys in confidence that Cerci's daughter would be wed to a
      Dothraki warlord. He told Littlefinger (again in confidence)
      that Cerci's daughter would be wed to a Dornish prince.

      When Cerci complained to Tyrion about the apocryphal news that
      her daughter would be forced to leave the capitol to marry,
      her destination told him which friend had betrayed his trust.
    "#
  }
}