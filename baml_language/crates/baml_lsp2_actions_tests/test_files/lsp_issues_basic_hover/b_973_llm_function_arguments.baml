class Email {
  subject string
}

client TestClient = openai.ResponsesClient.new(model = "gpt-4o-mini");

function ClassifyEmail(em<[CURSOR]ail: Email) -> string {
  client: TestClient
  prompt: `
    Subject: ${em<[CURSOR]ail.subject}
  `
}

//----
//- diagnostics
// <no-diagnostics-expected>
//
//- on_hover expressions
// hover at b_973_llm_function_arguments.baml:7:26
// ```baml
// email: Email
// ```
// hover at b_973_llm_function_arguments.baml:10:18
// ```baml
// email: Email
// ```
