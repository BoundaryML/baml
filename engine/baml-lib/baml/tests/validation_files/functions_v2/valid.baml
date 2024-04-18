client<llm> Bar {
  provider baml-openai-chat
}

function InputNone() -> int {
  client Bar
  prompt #"fa"#
}

function InputSimple(a: string, b: int | bool) -> int {
  client Bar
  prompt #"fa"#
}

class Email {
    subject string
    body string
}

function InputComplex(email: Email) -> string {
  client Bar
  prompt #" Given the email {{ email.subject }} "#
}
