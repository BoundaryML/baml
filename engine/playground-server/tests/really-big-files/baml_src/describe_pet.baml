class Pet {
  name string
  species string
  owner string
}

function DescribePet(pets: Pet[]) -> string {
  client "openai/gpt-4o"
  prompt #"
    Tell me what the owner of these pets is like (they own {{ pets|length }} pets):

    {% for pet in pets %}
    - {{ pet.name }} ({{ pet.species }}) owned by {{ pet.owner }}
    {% endfor %}

    {{ ctx.output_format }}
  "#
}

test simple_test {
  functions [DescribePet]
  args {
    pets [
      {
        name "Fido",
        species "Dog",
        owner "Sam"
      },
    ]
  }
}