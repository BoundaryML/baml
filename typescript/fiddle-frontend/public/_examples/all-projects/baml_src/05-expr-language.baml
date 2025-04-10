// `fn` functions are made of expressions, rather than a
// client and a prompt like normal BAML functions.
//
// This function combines the output from two separate
// LLM's and compares them using a third LLM as a judge.
fn RunPoemFaceoff(subject: string, length: int) -> Comparison {
  let claude_poem = CreateClaudePoem(subject, length);
  let gpt_poem = CreateOpenAIPoem(subject, length);
  ComparePoems(subject, claude_poem, gpt_poem)
}

test Faceoff {
  functions [RunPoemFaceoff]
  args {
    subject "love"
    length 10
  }
}

// Traditional BAML LLM-functions, wrapping prompts with the type
// signatures we know and love.
function CreateClaudePoem(subject: string, length: int) -> string {
  client Claude
  prompt #"Write a {{ length }} line poem about {{ subject }}."#
}

function CreateOpenAIPoem(subject: string, length: int) -> string {
  client GPT4
  prompt #"Write a {{ length }} line poem about {{ subject }}."#
}

class Comparison {
  poem_1_score int @description("1-10 rating of the first poem's quality")
  poem_2_score int @description("1-10 rating of the second poem's quality")
  reasoning string @description("Reasons for the above scores, explicitly contrasting the poems.")
}

function ComparePoems(subject: string, poem_1: string, poem_2: string) -> Comparison {
  client Claude
  prompt #"
    Two poets have written poems on the subject of {{ subject }}.
    Please grade and compare them according to the output schema.
    
    {{ ctx.output_format }}
  "#
}