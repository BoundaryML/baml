function TestAws(input: string) -> string {
  client AwsBedrock
  prompt #"
    Write a nice short story about {{ input }}. Keep it to 15 words or less.
  "#
}

/// my docs
class UniverseQuestion {
  question string
  answer string
}

class UniverseQuestionInput {
  question string
}

function TestUniverseQuestion(question: UniverseQuestionInput) -> UniverseQuestion {
  client AwsBedrock
  prompt #"
    You are a helpful assistant that answers questions about the universe.

    {{ ctx.output_format }}

    {{ _.role("user")}}

    Question: {{ question }}

    Answer:
  "#
}


function LongQuestion(prompt: string) -> UniverseQuestion {
  client AwsBedrock
  prompt #"
    {{ _.role("system")}}

    You are a helpful assistant that answers questions about the universe. Your knowledge is vast and your wisdom is deep. You have been entrusted with the task of enlightening the curious minds of the world. Your answers are not just words, they are the keys to unlocking the mysteries of the universe.

    {{ ctx.output_format }}

    {{ _.role("user")}}

    Question: {{ prompt }}
    Answer:
  "#
}

test TestLongQuestion {
  functions [LongQuestion]
  args {
    prompt #"
      What is the meaning of life, and how can we find purpose in our existence? This question has puzzled philosophers, theologians, and scientists for centuries, and yet, it remains one of the most profound and elusive questions of our time.

As we navigate the complexities of our lives, we often find ourselves searching for answers to this question. We may turn to religion, spirituality, or personal relationships to find meaning, but the answer remains elusive. Can you provide some insight into this question, and help us understand the human experience?
    "#
  }
}

function TestAwsInvalidRegion(input: string) -> string {
  client AwsBedrockInvalidRegion
  prompt #"
    Write a nice short story about {{ input }}. Keep it to 15 words or less.
  "#
}

function TestAwsInvalidAccessKey(input: string) -> string {
  client AwsBedrockInvalidAccessKey
  prompt #"
    Write a nice short story about {{ input }}. Keep it to 15 words or less.
  "#
}

function TestAwsInvalidProfile(input: string) -> string {
  client AwsBedrockInvalidProfile
  prompt #"
    Write a nice short story about {{ input }}. Keep it to 15 words or less.
  "#
}

function TestAwsInvalidSessionToken(input: string) -> string {
  client AwsBedrockInvalidSessionToken
  prompt #"
    Write a nice short story about {{ input }}. Keep it to 15 words or less.
  "#
}

function TestAwsInferenceProfile(input: string) -> string {
  client AwsBedrockInferenceProfileClient
  prompt #"
    Write a nice short story about {{ input }}. Keep it to 15 words or less.
  "#
}

// slow on purpose to try and trigger the stalled stream protection (which should be disabled)
function TestAwsClaude37(input: string) -> string {
  client AwsBedrockClaude37Client
  prompt #"
    Write 12 haikus. Number them.
  "#
}

test TestName {
  functions [TestAwsInferenceProfile]
  args {
    input #"
      hello world
    "#
  }
}


client<llm> AwsBedrockClaude37Client {
  provider "aws-bedrock"
  options {
    model "arn:aws:bedrock:us-east-1:404337120808:inference-profile/us.anthropic.claude-3-7-sonnet-20250219-v1:0"
    additional_model_request_fields {
      thinking {
        type "enabled"
        budget_tokens 1030
      }
    }
  }
}



client<llm> AwsBedrockInferenceProfileClient {
  provider "aws-bedrock"
  options {
    model "arn:aws:bedrock:us-east-1:404337120808:inference-profile/us.anthropic.claude-3-7-sonnet-20250219-v1:0"
  }
}
