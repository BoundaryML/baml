// single line comment

/* multi
   line
   comment
*/

class Resume {
  name string
  // inline field comment
  skills string[]
}

function ExtractResume(text: string) -> Resume {
  client "openai/gpt-4o"
  prompt #"
    Extract from: {{ text }}
  "#
}
