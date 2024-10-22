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

function ExtractPeople(text: string) -> Person[] {
  client GPT4
  prompt #"
     {{ _.role('system') }}
		 You are an expert extraction algorithm. Only extract relevant information from the text. If you do not know the value of an attribute asked to extract, return null for the attribute's value.
		 
		 {# This is a special macro that prints out the output schema of the function #}
		 {{ ctx.output_format }} 
		 
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


function ExtractHobby(text: string) -> Hobby[] {
  client GPT4
  prompt #"
     {{ _.role('system') }}
     {# This is a special macro that prints out the output schema of the function #}
     {{ ctx.output_format }} 
     
     {{ _.role('user') }}
     {{text}}
   "#
}