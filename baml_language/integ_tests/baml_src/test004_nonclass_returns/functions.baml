function ExtractKeywords(text: string) -> string[] {
  client GPT4oMini
  prompt #"
    Extract the most important keywords from the following text.
    Return a JSON array of strings.

    Text:
    {{ text }}

    JSON array:
  "#
}

function CountWords(text: string) -> int {
  client GPT4oMini
  prompt #"
    Count the number of words in the following text.
    Return only a single integer.

    Text:
    {{ text }}

    Answer:
  "#
}

function ExtractMultipleResumes(text: string) -> Test001Basic[] {
  client GPT4oMini
  prompt #"
    Extract all people mentioned in the text as resumes.
    Each resume should have a name and a list of skills.

    Respond in JSON as an array of objects:
    [{"name": "string", "skills": ["string", ...]}, ...]

    Text:
    {{ text }}

    JSON array:
  "#
}

function Summarize(text: string) -> string {
  client GPT4oMini
  prompt #"
    Summarize the following text in one sentence.

    Text:
    {{ text }}

    Summary:
  "#
}
