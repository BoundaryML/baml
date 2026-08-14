// This is a BAML file. Prompts are backtick templates with ${...} interpolations.
// Run a test to see how it works!

// https://docs.boundaryml.com

// We want the LLM to extract this info from an image receipt
class Receipt {
  establishment_name string
  date string @description("ISO8601 formatted date")
  total int @description("The total amount of the receipt")
  currency string
  items Item[] @description("The items on the receipt")
}

class Item {
  name string
  price float
  quantity int @description("If not specified, assume 1")
}

// This is our LLM function we can call in Python or Typescript
// the receipt can be an image OR text here!
function ExtractReceipt(receipt: image | string) -> Receipt {
  // see clients.baml
  client: GPT4o
  prompt: `
    Extract info from this receipt:
    ${receipt}

    ${ctx.output_format}
  `
}

// Test when the input is an image
test ImageReceiptTest {
  functions [ExtractReceipt]
  args {
    receipt { url "https://i.redd.it/adzt4bz4llfc1.jpeg"}
  }
}

// Test when the input is a string
test StarbucksTextReceiptTest {
  functions [ExtractReceipt]
  args {
    // use #""# for multi-line strings
    receipt #"
      Starbucks
      Date: 2022-01-01
      Total: $5.00 USD
      Items:
      - Coffee
        - $2.50
        - 1
      - Croissant
        - $2.50
        - 1
    "#
  }
}

class Resume {
  name string
  education Education[] @description("Extract in the same order listed")
  skills string[] @description("Only include programming languages")
}

class Education {
  school string
  degree string
  year int
}

function ExtractResume(resume_text: string) -> Resume {
  // see clients.baml
  client: GPT4o

  // Change the models or this text and watch the prompt preview change!
  prompt: `
    Parse the following resume and return a structured representation of the data in the schema below.

    Resume:
    ---
    ${resume_text}
    ---

    ${ctx.output_format}

    JSON:
  `
}

test Test1 {
  functions [ExtractResume]
  args {
    resume_text #"
      John Doe

      Education
      - University of California, Berkeley
        - B.S. in Computer Science
        - 2020

      Skills
      - Python
      - Java
      - C++
    "#
  }
}
// This will be available as an enum in your Python and Typescript code.
enum Category {
    Refund
    CancelOrder
    TechnicalSupport
    AccountIssue
    Question
}

class Message {
  userName string
  message string
}

// inputs can be more complex than just a string
function ClassifyMessage(message: Message) -> Category {
  client: GPT4o

  prompt: `
    Classify the following INPUT into ONE
    of categories listed.

    INPUT:
    ---
    ${message.userName}: ${message.message}
    ---

    ${ctx.output_format}

    Response:
  `
}

test Test1 {
  functions [ClassifyMessage]
  args {
    message {
      userName "Alice"
      message "I can't access my account using my login credentials. I havent received the promised reset password email. Please help."
    }
  }
}
// These are LLM clients you can use in your functions. Clients are constructed
// values now; reliability composes with ai.Retry / ai.Fallback / ai.RoundRobin.

client GPT4 = openai.OpenAiClient.new(
  model = "gpt-4",
  api_key = env.OPENAI_API_KEY,
);

// A client wrapped with a retry policy.
client GPT4o = ai.clients.Retry.new(
  inner = openai.OpenAiClient.new(model = "gpt-4o", api_key = env.OPENAI_API_KEY),
  max_attempts = 3,
);

client Claude = anthropic.AnthropicClient.new(
  model = "claude-3-haiku-20240307",
  api_key = env.ANTHROPIC_API_KEY,
  max_tokens = 1000,
);

client Gemini = google.GoogleClient.new(
  model = "gemini-2.5-flash",
  api_key = env.GOOGLE_API_KEY,
);

// You can use a fallback to add more resilience to your application.
// A fallback will attempt to use the first client, and if it fails, it will
// try the second client, and so on.
// https://docs.boundaryml.com/docs/snippets/clients/fallback
client SuperDuperClient = ai.clients.Fallback {
  members: [
    GPT4,
    GPT4o,
    Gemini,
  ],
};

// An example of a BAML client with a retry policy
// https://docs.boundaryml.com/docs/snippets/clients/retry
client MyClient = ai.clients.Retry.new(
  inner = anthropic.AnthropicClient.new(
    model = "claude-3-sonnet-20240229",
    api_key = env.ANTHROPIC_API_KEY,
  ),
  max_attempts = 3,
);

// Round robin distributes requests across multiple clients. After each call,
// the next client in the list will be used.
client MyRoundRobinClient = ai.clients.RoundRobin.new([
  GPT4o,
  Gemini,
]);

class WeatherAPI {
  city string @description("the user's city")
  timeOfDay string @description("As an ISO8601 timestamp")
}

function UseTool(user_message: string) -> WeatherAPI {
  client: GPT4o
  prompt: `
    Extract the info from this message
    ---
    ${user_message}
    ---

    ${ctx.output_format}

    JSON:
  `
}

test TestOne {
  functions [UseTool]
  args {
    user_message #"
      what's the weather in san francisco tomorrow april 23rd 2024 at 3pm?
    "#
  }
}

class WeatherAPI2 {
  city string @description("the user's city")
  timeOfDay string @description("As an ISO8601 timestamp")
}

class SendEmail {
  emailTo string
  emailSubject string
  emailBody string
}

function ChooseOneTool(user_message: string) -> WeatherAPI2 | SendEmail {
  client: GPT4o
  prompt: `
    Choose the right schema that contains all the information in this message:
    ---
    ${user_message}
    ---

    ${ctx.output_format}

    JSON:
  `
}

test TestOneFunc {
  functions [ChooseOneTool]
  args {
    user_message #"
      what's the weather in san francisco tomorrow april 23rd 2024 at 3pm?
    "#
  }
}

test TestOneFunc2 {
  functions [ChooseOneTool]
  args {
    user_message #"
      Send an email to John Doe with the subject 'Hello' and the body 'How are you doing?'
    "#
  }
}


function ChooseNTools(user_message: string) -> (WeatherAPI2 | SendEmail)[] {
  client: GPT4o
  prompt: `
    Choose the right schema(s) that contains all the information in this message:
    ---
    ${user_message}
    ---

    ${ctx.output_format}

    JSON:
  `
}

test TestNTools {
  functions [ChooseNTools]
  args {
    user_message #"
      what's the weather in san francisco tomorrow april 23rd 2024 at 3pm?

      Also send an email to Mark Gonzalez with the subject 'Hello' and the body 'How are you doing?'
    "#
  }
}

// BAML supports multi-modal inputs!

function AudioInput(audioInput: audio) -> string {
  client: Gemini
  prompt: `
   Does this sound like a roar? Yes or no? One word no other characters.

    ${audioInput}
  `
}


test TestURLAudioInput{
  functions [AudioInput]
  args {
    audioInput {
      url "https://actions.google.com/sounds/v1/emergency/beeper_emergency_call.ogg"
    }
  }
}

class ImageDescription {
  description string
  tags string[] @description(#"
    Tags that describe the image
  "#)
}

function DescribeImage(myImage: image) -> ImageDescription {
  client: GPT4o
  prompt: `
    Describe this in 2 sentences: ${myImage}

    ${ctx.output_format}
  `
}


test ImageTest {
  functions [DescribeImage]
  args {
    myImage { url "https://imgs.xkcd.com/comics/standards.png"}
  }
}

// BAML supports multi-modal inputs including Pdfs!

function PdfInput(pdfInput: pdf) -> string {
  client: GPT4o
  prompt: `
    Get the title of the pdfs

    ${pdfInput}
  `
}

test TestFilePdfInput {
  functions [PdfInput]
  args {
    pdfInput {
      url "https://www.w3.org/WAI/ER/tests/xhtml/testfiles/resources/pdf/dummy.pdf"
    }
  }
}
// BAML supports multi-modal inputs including videos!
// Note: Video input only works with Google models (Gemini)

function VideoInput(videoInput: video) -> string {
  client: Gemini
  prompt: `
    Describe what happens in this video in 5 words or less.

    ${videoInput}
  `
}

test TestFileVideoInput {
  functions [VideoInput]
  args {
    videoInput {
      url "https://www.youtube.com/watch?v=dQw4w9WgXcQ"
      media_type "video/mp4"
    }
  }
}
class OResume {
  name string
  education OEducation[] @description("Extract in the same order listed")
  skills string[] @description("Only include programming languages")
}

class OEducation {
  school string
  degree string
  year int
}

function ExtractResumeUsingOllama(resume_text: string) -> OResume {
  // see ollama-clients.baml
  client: Llama3
  // client Mistral
  // client Gemma2
  // client Phi3

  prompt: `
    Parse the following resume and return a structured representation of the data in the schema below.

    Resume:
    ---
    ${resume_text}
    ---

    ${ctx.output_format}

    JSON:
  `
}

test OllamaTest1 {
  functions [ExtractResumeUsingOllama]
  args {
    resume_text #"
      John Doe

      Education
      - University of California, Berkeley
        - B.S. in Computer Science
        - 2020

      Skills
      - Python
      - Java
      - C++
    "#
  }
}
enum OCategory {
    Refund
    CancelOrder
    TechnicalSupport
    AccountIssue
    Question
}

class OMessage {
  userName string
  message string
}

// inputs can be more complex than just a string
// This will be available as an enum in your Python and Typescript code.
// inputs can be more complex than just a string
function ClassifyMessageUsingOllama(message: OMessage) -> OCategory {
  client: Llama3
  // client Mistral
  // client Gemma2
  // client Phi3

  prompt: `
    Classify the following INPUT into ONE
    of categories listed.

    INPUT:
    ---
    ${message.userName}: ${message.message}
    ---

    ${ctx.output_format}

    Response:
  `
}

test OllamaTest1 {
  functions [ClassifyMessageUsingOllama]
  args {
    message {
      userName "Alice"
      message "I can't access my account using my login credentials. I havent received the promised reset password email. Please help."
    }
  }
}
// Local Ollama models speak the OpenAI wire protocol, so they are constructed
// OpenAI clients pointed at the local server.
//
// Note that you should run ollama using `OLLAMA_ORIGINS='*' ollama serve`
// and you'll also need to `ollama pull llama3` to use these clients.

client Llama3 = openai.OpenAiClient.new(
  model = "llama3",
  base_url = "http://localhost:11434/v1",
);

client Mistral = openai.OpenAiClient.new(
  model = "mistral",
  base_url = "http://localhost:11434/v1",
);

client Gemma2 = openai.OpenAiClient.new(
  model = "gemma2",
  base_url = "http://localhost:11434/v1",
);

client Phi3 = openai.OpenAiClient.new(
  model = "phi3",
  base_url = "http://localhost:11434/v1",
);

// BAML excels at parsing the schemas from LLM responses.

// In this example we tell the model to think step by step and explain its reasoning
// before outputting the answer.

class Email {
    subject string
    body string
    from_address string
}

enum OrderStatus {
    ORDERED
    SHIPPED
    DELIVERED
    CANCELLED
}

class OrderInfo {
    order_status OrderStatus
    tracking_number string?
    estimated_arrival_date string?
}

function GetOrderInfo(email: Email) -> OrderInfo {
  client: GPT4o
  prompt: `
    Extract the info from this email in the INPUT:

    INPUT:
    -------
    from: ${email.from_address}
    Email Subject: ${email.subject}
    Email Body: ${email.body}
    -------

    ${ctx.output_format}

    Before you output the JSON, please explain your
    reasoning step-by-step. Here is an example on how to do this:
    'If we think step by step we can see that ...
     therefore the output JSON is:
    {
      ... the json schema ...
    }'
  `
}

test Test1 {
  functions [GetOrderInfo]
  args {
    email {
      from_address "hello@amazon.com"
      subject "Your Amazon.com order of 'Wood Dowel Rods...' has shipped!"
      body #"
        Hi Sam, your package will arrive:
        Thurs, April 4
        Track your package:
        www.amazon.com/gp/your-account/ship-track?ie=23&orderId123

        On the way:
        Wood Dowel Rods...
        Order #113-7540940
        Ship to:
            Sam
            SEATTLE, WA

        Shipment total:
        $0.00
    "#

    }
  }
}
// You can improve results by aliasing field names to symbols like "k1" ... "kN".
// This makes the LLM pay attention to your descriptions instead of just the enum or property name, since those can introduce biases. See paper: https://arxiv.org/abs/2305.08298 for more details.

// When you add aliases you don't need to change your resulting python code. Your models
// stay the same. They are merely a way to aid in prompt engineering.
enum MyClass {
    Refund @alias("k1")
    @description("Customer wants to refund a product")

    CancelOrder @alias("k2")
    @description("Customer wants to cancel an order")

    TechnicalSupport @alias("k3")
    @description("Customer needs help with a technical issue unrelated to account creation or login")

    AccountIssue @alias("k4")
    @description("Specifically relates to account-login or account-creation")

    Question @alias("k5")
    @description("Customer has a question")
}

function ClassifyMessageWithSymbol(input: string) -> MyClass {
  client: GPT4o

  prompt: `
    Classify the following INPUT into ONE
    of the following categories:

    INPUT: ${input}

    ${ctx.output_format}

    Response:
  `
}

test Test1 {
  functions [ClassifyMessageWithSymbol]
  args {
    input "I can't access my account using my login credentials. I havent received the promised reset password email. Please help."
  }
}
// This will be available as an enum in your Python and Typescript code.
enum MyCategory {
    Refund
    CancelOrder
    TechnicalSupport
    AccountIssue
    Question
}

// Prompts are instructions-only now; role markers are gone.
function ClassifyMessageWithRoles(input: string) -> MyCategory {
  client: GPT4o

  prompt: `
    Classify the following INPUT into ONE
    of the following categories:

    ${ctx.output_format}

    INPUT: ${input}

    Response:
  `
}

test Test1 {
  functions [ClassifyMessageWithRoles]
  args {
    input "Can't access my account using my usual login credentials, and each attempt results in an error message stating 'Invalid username or password.' I have tried resetting my password using the 'Forgot Password' link, but I haven't received the promised password reset email."
  }
}
class User {
  name string
  is_active bool
}

// Jinja conditionals are gone; interpolate the flag and let the model decide.
function FunctionWithConditionals(user: User) -> string {
  client: GPT4o
  prompt: `
    The user is ${user.name}. Active: ${user.is_active}.
    If the user is active, greet them; otherwise tell them to activate
    their account.
  `
}
class MyUserMessage {
  user_name string
  content string
}

// Jinja loops are gone; interpolate the list directly.
function FunctionWithLoops(messages: MyUserMessage[]) -> string {
  client: GPT4o
  prompt: `
    ${messages}
  `
}

test TestName {
  functions [FunctionWithLoops]
  args {
    messages [
      {
        user_name "Alice"
        content "Hello!"
      }
      {
        user_name "Bob"
        content "Hi!"
      }
    ]
  }
}

class UserMessage {
  message string
  role string
}

function ClassifyConversation(messages: UserMessage[]) -> Category[] {
  client: GPT4o
  prompt: `
    Classify this conversation:
    ${messages}

    Use the following categories:
    ${ctx.output_format}
  `
}

test TestClassifyConvo {
  functions [ClassifyConversation]
  args {
    messages [
      {
        role "user"
        message "I can't access my account using my login credentials. I havent received the promised reset password email. Please help."
      }
      {
        role "assistant"
        message "I'm sorry to hear that. Let me help you with that."
      }
      {
        role "user"
        message "Thank you."
      }
    ]
  }
}

//----
//- diagnostics
// <no-diagnostics-expected>
