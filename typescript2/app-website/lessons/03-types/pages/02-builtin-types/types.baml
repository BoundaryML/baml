function SpeechToText(in: audio) -> string[] {
  client GPT4
  prompt #"
    What is said in this audio file?
    {{ in }}
    Use one string output per sentence.
    {{ ctx.output_format }}
  "#
}

function ArithmeticPoemThemes(i: int, f: float, poem: string) -> map<string,string> {
  client GPT4
  prompt #"
    The following poem was written to explore the relationship
    between {{ i }} and {{ f }}.

    Extract the themes of the poem. For each theme, describe what the
    poem has to say on this theme.

    {{ ctx.output_format }}
  "#
}

test Themes {
  functions [ArithmeticPoemThemes]
  args {
    i 2
    f 1000.0
    poem #"
      We two, a thousand became.
      Mirrored in one, the other.
      Our love multiplied.
      We, into the fog.
      But the love diminished none.
    "#
  }
}