class Character {
  name string @assert({{ this|length > 0 }}) @check(is_tyrion, {{ this == "Tyrion" }})
  age int? @description("Approximate age. Use null if you have no good guess")
  characteristics Characteristic[] @description("Order by salience descending")
  relationships map<string,string> @description("map from other character to this character's rln to them")
}

enum Characteristic {
  Kind
  Mean @skip() // Legacy - don't extract these.
  Daring
  Cunning
  Noble @alias("Esteemed") // "Noble" cofsuses small models.
}

class CharacterList {
  items Character[] @description("List characters ")
}

function ExtractCharacters(story: string) -> CharacterList {
   client GPT4
   prompt #"
    Extract the Characters from the given story. Order them by importance:
    {{ story }}
    {{ ctx.output_format }}
   "#
}