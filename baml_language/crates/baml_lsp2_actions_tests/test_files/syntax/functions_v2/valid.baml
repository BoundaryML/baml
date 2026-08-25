client Bar = openai.ResponsesClient.new(model = "gpt-4o");

function InputNone() -> int {
  client: Bar
  prompt: `fa`
}

function InputSimple(a: string, b: int | bool) -> int {
  client: Bar
  prompt: `fa`
}

class Email {
    subject string
    body string
}

function InputObject(email: Email) -> string {
  client: Bar
  prompt: ` subject line ${email.subject} `
}

enum Color {
  RED
  BLUE
  GREEN
}

function InputEnum(color: Color) -> string {
  client: Bar
  prompt: ` color ${color} `
}

// Try it with wierd spacing and comments

function InputEnum2(
  color: Color,
  bar: string, // ok

  // This param is great!
  foo: string[],

  // But we should do something...
  baz: (string | int?)[]
)
// Err
->
// Do something
string

// Or else
{
  client: Bar
  prompt: ` color ${color} `
}


function InputImage(img: image) -> string {
  client: Bar
  prompt: ` image ${img} `
}

// The old Jinja enum-comparison surface is gone; prompts are plain backtick
// templates now.
function TestEnumStringEquality(color: Color) -> string {
  client: Bar
  prompt: `
    The color is: ${color}

    ${ctx.output_format()}
  `
}

function TestEnumInExpressions(color: Color) -> string {
  client: Bar
  prompt: `
    Classify the color ${color}.

    ${ctx.output_format()}
  `
}

//----
//- diagnostics
// <no-diagnostics-expected>
