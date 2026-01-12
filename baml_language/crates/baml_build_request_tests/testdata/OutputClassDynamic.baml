client<llm> GPT4 {
  provider openai
  options {
    model gpt-4o
    api_key env.OPENAI_API_KEY
  }
}

class Person {
  name string?
  hair_color Color?

  @@dynamic
}

enum Color {
  RED
  BLUE
  GREEN
  YELLOW
  BLACK
  WHITE

  @@dynamic
}

function FnOutputClassDynamic(text: string) -> Person[] {
  client GPT4
  prompt #"
     {{ _.role('system') }}
		 You are an expert extraction algorithm. Only extract relevant information from the text. If you do not know the value of an attribute asked to extract, return null for the attribute's value.

		 {# This is a special macro that prints out the output schema of the function #}
		 {{ ctx.output_format() }}

		 {{ _.role('user') }}
		 {{text}}
   "#
}

enum Hobby {
  SPORTS
  MUSIC
  READING

  @@dynamic
}

test TestOutputClassDynamic {
  functions [FnOutputClassDynamic]
  args {
    text "example text"
  }
}
