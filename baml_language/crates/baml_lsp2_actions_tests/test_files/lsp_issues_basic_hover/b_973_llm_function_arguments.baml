class Email {
  subject string
}

client<llm> TestClient {
  provider openai
  options {
    model "gpt-4o-mini"
  }
}

function ClassifyEmail(em<[CURSOR]ail: Email) -> string {
  client TestClient
  prompt #"
    Subject: {{ em<[CURSOR]ail.subject }}
  "#
}

//----
//- diagnostics
// <no-diagnostics-expected>
//
//- on_hover expressions
// hover at b_973_llm_function_arguments.baml:12:26
// ```baml
// email: Email
// ```
// hover at b_973_llm_function_arguments.baml:15:19
// ```baml
// email: Email
// ```
