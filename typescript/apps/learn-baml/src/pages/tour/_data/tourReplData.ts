export interface TourReplExample {
  code: string;
  functionName: string;
  args: string;
  tryIt?: string;
  challenge?: string;
}

export const tourReplExamples: Record<string, TourReplExample> = {
  'hello-baml': {
    code: `enum Sentiment {
  POSITIVE
  NEGATIVE
  NEUTRAL
}

function ClassifySentiment(text: string) -> Sentiment {
  client "openai/gpt-4o-mini"
  prompt #"
    Classify the sentiment of: {{ text }}
    {{ ctx.output_format }}
  "#
}`,
    functionName: 'ClassifySentiment',
    args: `{
  "text": "I absolutely love this product! Best purchase ever!"
}`,
    challenge: 'Change the enum to HAPPY/SAD/NEUTRAL and run again to see prompt + output adapt automatically.',
    tryIt: 'You are editing real BAML here. No generated repo required.',
  },

  'prompt-transparency': {
    code: `class Email {
  subject string
  body string
  priority "urgent" | "normal" | "low"
}

function DraftReply(original: Email, tone: "formal" | "casual") -> string {
  client "openai/gpt-4o-mini"
  prompt #"
    You received this email:
    Subject: {{ original.subject }}
    Body: {{ original.body }}
    Priority: {{ original.priority }}

    Write a {{ tone }} reply.
  "#
}`,
    functionName: 'DraftReply',
    args: `{
  "original": {
    "subject": "Q3 Budget Review Meeting",
    "body": "Hi team, we need to discuss the Q3 budget allocations. Several departments have exceeded their limits.",
    "priority": "urgent"
  },
  "tone": "formal"
}`,
    challenge: 'Add a Jinja conditional for urgent emails and check how it changes the rendered prompt.',
  },

  'types-at-work': {
    code: `class ContactInfo {
  name string
  email string?
  phone string?
  company string?
}

function ExtractContact(text: string) -> ContactInfo {
  client "openai/gpt-4o-mini"
  prompt #"
    Extract contact information from:
    {{ text }}

    {{ ctx.output_format }}
  "#
}`,
    functionName: 'ExtractContact',
    args: `{
  "text": "Call John Smith at john@acme.com or 555-1234"
}`,
    challenge: 'Add an optional field (for example `title string?`) and run to see schema-guided output update.',
  },

  'union-types': {
    code: `class ContactInfo {
  name string
  email string?
  phone string?
}

class SuccessfulExtraction {
  data ContactInfo
  confidence float @description("0.0 to 1.0")
}

class NeedsMoreInfo {
  question string @description("What clarification is needed")
}

class CannotProcess {
  reason string
}

function SmartExtract(text: string) -> SuccessfulExtraction | NeedsMoreInfo | CannotProcess {
  client "openai/gpt-4o-mini"
  prompt #"
    Extract contact info from: {{ text }}

    If the text is clear, return the data with confidence score.
    If you need clarification, ask a specific question.
    If you cannot process it, explain why.

    {{ ctx.output_format }}
  "#
}`,
    functionName: 'SmartExtract',
    args: `{
  "text": "Call my friend tomorrow"
}`,
    challenge: 'Run with both ambiguous and explicit inputs and compare which union variant returns.',
  },

  attributes: {
    code: `class Person {
  name string @description("Full legal name")
  email string? @description("Primary email address")
  dob string @alias("date_of_birth") @description("Format: YYYY-MM-DD")
  ssn string? @alias("social_security") @skip
}

enum Priority {
  Urgent @alias("P0") @description("Immediate attention required")
  High @alias("P1")
  Normal @alias("P2")
  Low @alias("P3") @description("Can wait until next sprint")
}

function ExtractPerson(text: string) -> Person {
  client "openai/gpt-4o-mini"
  prompt #"
    Extract person information from:
    {{ text }}

    {{ ctx.output_format }}
  "#
}`,
    functionName: 'ExtractPerson',
    args: `{
  "text": "Contact Jane Doe (born March 15, 1990) at jane.doe@company.com. Her SSN is 123-45-6789."
}`,
    challenge: 'Temporarily remove `@skip` from `ssn` and inspect how the prompt and output contract change.',
  },

  'testing-loop': {
    code: `enum Intent {
  QUESTION
  COMPLAINT
  FEEDBACK
  OTHER
}

function ClassifyIntent(message: string) -> Intent {
  client "openai/gpt-4o-mini"
  prompt #"
    Classify the customer intent:
    {{ message }}
    {{ ctx.output_format }}
  "#
}

test ClassifyIntentQuestion {
  functions [ClassifyIntent]
  args {
    message "How do I reset my password?"
  }
  @@assert( {{ this == Intent.QUESTION }} )
}

test ClassifyIntentComplaint {
  functions [ClassifyIntent]
  args {
    message "This product broke after one day!"
  }
  @@assert( {{ this == Intent.COMPLAINT }} )
}`,
    functionName: 'ClassifyIntent',
    args: `{
  "message": "How do I reset my password?"
}`,
    challenge: 'Add a third test case and an `@@assert(...)`, then iterate on prompt instructions until it passes consistently.',
  },

  'match-expressions': {
    code: `class ContactInfo {
  name string
  email string?
  phone string?
}

class SuccessfulExtraction {
  data ContactInfo
}

class NeedsMoreInfo {
  question string
}

class CannotProcess {
  reason string
}

function SmartExtract(text: string) -> SuccessfulExtraction | NeedsMoreInfo | CannotProcess {
  client "openai/gpt-4o-mini"
  prompt #"
    Extract contact info from: {{ text }}

    If the text is clear, return data.
    If clarification is needed, ask a question.
    If the text is not processable, explain why.

    {{ ctx.output_format }}
  "#
}

test AmbiguousInput {
  functions [SmartExtract]
  args {
    text "Call my friend"
  }
  @@assert( {{ this is NeedsMoreInfo }} )
}`,
    functionName: 'SmartExtract',
    args: `{
  "text": "Call my friend"
}`,
    challenge: 'Add a new union variant and update test assertions to keep downstream handling explicit and exhaustive.',
  },

  streaming: {
    code: `class Article {
  title string
  summary string @description("2-3 sentence summary")
  keyPoints string[] @description("3-5 bullet points")
  sentiment Sentiment
}

enum Sentiment {
  POSITIVE
  NEGATIVE
  NEUTRAL
}

function SummarizeArticle(url: string) -> Article {
  client "openai/gpt-4o-mini"
  prompt #"
    Summarize the article at: {{ url }}
    {{ ctx.output_format }}
  "#
}`,
    functionName: 'SummarizeArticle',
    args: `{
  "url": "https://example.com/tech-news"
}`,
    challenge: 'Add one more field (like `riskLevel`) and observe typed schema evolution in one run.',
  },

  'client-strategies': {
    code: `retry_policy Resilient {
  max_retries 3
  strategy {
    type exponential_backoff
    initial_delay_ms 500
    max_delay_ms 10000
  }
}

client<llm> ReliableGPT {
  provider openai
  options {
    model "gpt-4o-mini"
    api_key env.OPENAI_API_KEY
  }
  retry_policy Resilient
}

function DraftSupportReply(message: string) -> string {
  client ReliableGPT
  prompt #"
    Write a concise, empathetic support response to this customer message:
    {{ message }}
  "#
}`,
    functionName: 'DraftSupportReply',
    args: `{
  "message": "I was charged twice and I need help today."
}`,
    challenge: 'Create a second client and swap the function to it to compare behavior without rewriting business logic.',
  },

  'polyglot-integration': {
    code: `class SentimentBreakdown {
  positive int
  negative int
  neutral int
}

function AnalyzeSentiment(reviews: string[]) -> SentimentBreakdown {
  client "openai/gpt-4o-mini"
  prompt #"
    Analyze these reviews:
    {% for review in reviews %}
    - {{ review }}
    {% endfor %}

    Count how many are positive, negative, and neutral.
    {{ ctx.output_format }}
  "#
}`,
    functionName: 'AnalyzeSentiment',
    args: `{
  "reviews": [
    "Great product!",
    "Terrible service",
    "It's okay I guess"
  ]
}`,
    challenge: 'Change the output type to include a `mixed` bucket and validate that callers would get the new typed field.',
  },

  'dynamic-types': {
    code: `class FormField {
  label string
  value string
  @@dynamic
}

function ExtractFormFields(formText: string) -> FormField[] {
  client "openai/gpt-4o-mini"
  prompt #"
    Extract all form fields from this input:
    {{ formText }}

    {{ ctx.output_format }}
  "#
}`,
    functionName: 'ExtractFormFields',
    args: `{
  "formText": "Email: user@example.com\\nCompany: Boundary\\nRole: Product Engineer"
}`,
    challenge: 'Add a field to `FormField`, then run to confirm the output contract updates instantly.',
  },

  'putting-it-together': {
    code: `class CustomerMessage {
  content string
  customerTier "enterprise" | "pro" | "free"
  previousTickets int
}

class DraftResponse {
  response string
  suggestedActions string[]
  confidence float
}

class EscalateToHuman {
  reason string
  urgency "immediate" | "high" | "normal"
  suggestedTeam "billing" | "technical" | "legal" | "general"
}

class NeedMoreContext {
  questions string[]
}

function TriageMessage(msg: CustomerMessage) -> DraftResponse | EscalateToHuman | NeedMoreContext {
  client "openai/gpt-4o-mini"
  prompt #"
    You are a customer support assistant.

    Customer tier: {{ msg.customerTier }}
    Previous tickets: {{ msg.previousTickets }}
    Message: {{ msg.content }}

    {% if msg.customerTier == "enterprise" %}
    Enterprise customers should be escalated when issue complexity is high.
    {% endif %}

    {{ ctx.output_format }}
  "#
}

test EnterprisePriority {
  functions [TriageMessage]

  args {
    msg {
      content "Our API is down and we're losing revenue"
      customerTier "enterprise"
      previousTickets 0
    }
  }

  @@assert( {{ this is EscalateToHuman }} )
}`,
    functionName: 'TriageMessage',
    args: `{
  "msg": {
    "content": "Our API is down and we're losing revenue",
    "customerTier": "enterprise",
    "previousTickets": 0
  }
}`,
    challenge: 'Add a new tier or escalation rule, then run and verify the union branch changes as expected.',
    tryIt: 'You now have the same loop you would use in production: edit schema, run, inspect prompt, inspect typed output.',
  },
};
