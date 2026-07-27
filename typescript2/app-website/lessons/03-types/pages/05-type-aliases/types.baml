class PhoneNumber {
  digits int
}

class Email {
  address string
}

class LinkedInHandle {
  handle string
}

type ContactInfo = PhoneNumber | Email | LinkedInHandle

// Represent any JSON value.
type JSON = float | int | bool | string | JSON[] | map<string, JSON>

function UnstructuredDoc(in: string) -> JSON {
  client GPT4
  prompt #"
    Extract the input string into whatever unstructured
    JSON document makes the most sense for the content.

    {{ ctx.output_format }}
  "#
}

test UnstructuredDoc {
  functions [UnstructuredDoc]
  args {
    in #"
      
    "#
  }
}