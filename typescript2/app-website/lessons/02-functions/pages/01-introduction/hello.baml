function Greet(name: string) -> string {
    client GPT4
    prompt #"
      Generate a casual greeting for the user named {{ name }}.
      The greeting should be friendly, informal, and less that
      10 words long.
    "#
}

test TestGreet {
  functions [Greet]
  args {
    name "Mindy"
  }
}