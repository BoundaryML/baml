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

test TestName {
  functions [TestAwsInferenceProfile]
  args {
    input #"
      hello world
    "#
  }
}


client<llm> AwsBedrockInferenceProfileClient {
  provider "aws-bedrock"
  options {
    model "arn:aws:bedrock:us-east-1:404337120808:inference-profile/us.anthropic.claude-3-7-sonnet-20250219-v1:0"
  }
}
