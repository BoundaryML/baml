import { TestRunOutput } from "@/app/[project_id]/_atoms/atoms";
import { EditorFile } from "@/app/actions";
import { ParserDatabase, StringSpan } from "@baml/common";

// export type ParserDBFunctionTestModel = Pick<ParserDatabase['functions'][0], 'name' | 'test_cases'>;

export type BAMLProject = {
  id: string;
  name: string;
  description: string;
  files: EditorFile[];
  // functionsWithTests: ParserDBFunctionTestModel[];
  testRunOutput?: TestRunOutput;
};

const extractResumeBaml = `// This is a BAML config file, which extends the Jinja2 templating language to write LLM functions.

// BAML adds many new features to Jinja:
// - type-support,
// - static analysis of prompts, 
// - robust deserialization of JSON outputs,
// - ...and more! 

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
  client GPT4Turbo

  // The prompt uses Jinja syntax
  prompt #"
    Parse the following resume and return a structured representation of the data in the schema below.

    Resume:
    ---
    {{ resume_text }}
    ---

    {# special macro to print the output instructions. #}
    {{ ctx.output_format }}

    JSON:
  "#
}
// Open main.py to see how to use this function
`;

const extractResumeTest = {
  "resume_text": "Jason Doe\nPython, Rust\nUniversity of California, Berkeley, B.S.\nin Computer Science, 2020\nAlso an expert in Tableau, SQL, and C++\n"
};

const classifyMessageBaml = `// This will be available as an enum in your Python and Typescript code.
enum Category {
    Refund
    CancelOrder
    TechnicalSupport
    AccountIssue
    Question
}

function ClassifyMessage(input: string) -> Category {
  client GPT4Turbo

  prompt #"
    Classify the following INPUT into ONE
    of the following categories:

    INPUT: {{ input }}

    {{ ctx.output_format }}

    Response:
  "#
}
`;

const classifyMessageTest = {
  "input": "Can't access my account using my usual login credentials, and each attempt results in an error message stating \"Invalid username or password.\" I have tried resetting my password using the 'Forgot Password' link, but I haven't received the promised password reset email."
}

const chainOfThoughtBaml = `
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
  client GPT4Turbo
  prompt #"
    Given the email below:

    \`\`\`
    from: {{email.from_address}}
    Email Subject: {{email.subject}}
    Email Body: {{email.body}}
    \`\`\`

    Extract this info from the email in JSON format:
    {{ ctx.output_format }}

    Before you output the JSON, please explain your
    reasoning step-by-step. Here is an example on how to do this:
    'If we think step by step we can see that ...
     therefore the output JSON is:
    {
      ... the json schema ...
    }'
  "#
}
`;

const chainOfThoughtTest = {
  "email": {
    "subject": "Your Amazon.com order of \"Wood Square Dowel Rods...\" has shipped!",
    "body": "Amazon Shipping Confirmation\nwww.amazon.com?ie=UTF8&ref_=scr_home\n\nHi Samuel, your package will arrive:\n\nThursday, April 4\n\nTrack your package:\nwww.amazon.com/gp/your-account/ship-track?ie=UTF8&orderId=113-7540940-3785857&packageIndex=0&shipmentId=Gx7wk71F9&ref_=scr_pt_tp_t\n\nOn the way:\nWood Square Dowel Rods...\nOrder #113-7540940-3785857\n\nAn Amazon driver may contact you by text message or call you for help on the day of delivery.    \n\nShip to:\n    Sam\n    SEATTLE, WA\n\nShipment total:\n$0.00",
    "from": "inov-8 <enquiries@inov-8.com>",
    "from_address": "\"Amazon.com\" <shipment-tracking@amazon.com>"
  }
};

const chatRolesBaml = `
// This will be available as an enum in your Python and Typescript code.
enum Category {
    Refund
    CancelOrder
    TechnicalSupport
    AccountIssue
    Question
}

function ClassifyMessage(input: string) -> Category {
  client GPT4Turbo

  prompt #"
    {# _.chat("system") starts a system message #}
    {{ _.chat("system") }}

    Classify the following INPUT into ONE
    of the following categories:

    {{ ctx.output_format }}

    {# This starts a user message #}
    {{ _.chat("user") }}

    INPUT: {{ input }}

    Response:
  "#
}
`;

const symbolTuningBaml = `
enum Category {
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

function ClassifyMessage(input: string) -> Category {
  client GPT4Turbo

  prompt #"
    Classify the following INPUT into ONE
    of the following categories:

    INPUT: {{ input }}

    {{ ctx.output_format }}

    Response:
  "#
}
`;

const clientsBaml = `// These are LLM clients you can use in your functions. We currently support Anthropic, OpenAI / Azure, and Ollama as providers but are expanding to many more.

// For this playground, we have setup a few clients for you to use already with some free credits.

client<llm> GPT4 {
  // Use one of the following: https://docs.boundaryml.com/v3/syntax/client/client#providers
  provider baml-openai-chat
  // You can pass in any parameters from the OpenAI Python documentation into the options block.
  options {
    model gpt-4
    api_key env.OPENAI_API_KEY
  }
} 

client<llm> GPT4Turbo {
  provider baml-openai-chat
  options {
    model gpt-4-1106-preview
    api_key env.OPENAI_API_KEY
  }
} 

client<llm> GPT35 {
  provider baml-openai-chat
  options {
    model gpt-3.5-turbo
    api_key env.OPENAI_API_KEY
  }
}  

client<llm> Claude {
  provider baml-anthropic-chat
  options {
    model claude-3-haiku-20240307
    api_key env.ANTHROPIC_API_KEY
  }
}
`;



export const exampleProjects: BAMLProject[] = [
  {
    id: 'extract-resume',
    name: 'Introduction to BAML',
    description: 'A simple LLM function extract a resume',
    files: [
      {
        path: 'baml_src/main.baml',
        content: extractResumeBaml,
      },
      {
        path: 'baml_src/clients.baml',
        content: clientsBaml,
      },
      {
        path: 'baml_src/__tests__/ExtractResume/test1.json',
        content: JSON.stringify({ input: extractResumeTest }),
      },
      {
        path: 'main.py',
        content: `from baml_client import baml as b
# BAML types get converted to Pydantic models
from baml_client.baml_types import Resume

async def main():
    resume_text = """Jason Doe\nPython, Rust\nUniversity of California, Berkeley, B.S.\nin Computer Science, 2020\nAlso an expert in Tableau, SQL, and C++\n"""

    # this function comes from the autogenerated "baml_client".
    # It calls the LLM you specified and handles the parsing.
    resume = await b.ExtractResume(resume_text)
    
    # Fully type-checked and validated!
    assert isinstance(resume, Resume)
`
      },
      {
        path: 'main.ts',
        content: `import b from 'baml_client'

async function main() {
    const resume_text = \`Jason Doe\nPython, Rust\nUniversity of California, Berkeley, B.S.\nin Computer Science, 2020\nAlso an expert in Tableau, SQL, and C++\n\`

    // this function comes from the autogenerated "baml_client".
    // It calls the LLM you specified and handles the parsing.
    const resume = await b.ExtractResume(resume_text)
    
    // Fully type-checked and validated!
    assert resume.name === "Jason Doe"
}
`
      }
    ]
  },
  {
    id: 'chain-of-thought',
    name: 'Chain of thought',
    description: 'Classify a message from a user, and demonstrate chain of thought',
    files: [
      {
        path: 'baml_src/main.baml',
        content: chainOfThoughtBaml,
      },
      {
        path: 'baml_src/clients.baml',
        content: clientsBaml,
      },
      {
        path: 'baml_src/__tests__/GetOrderInfo/test1.json',
        content: JSON.stringify({ input: chainOfThoughtTest }),
      },
    ]
  },
  {
    id: 'classify-message',
    name: 'ClassifyMessage',
    description: 'Classify a message from a user',
    files: [
      {
        path: 'baml_src/main.baml',
        content: classifyMessageBaml,
      },
      {
        path: 'baml_src/clients.baml',
        content: clientsBaml,
      },
      {
        path: 'baml_src/__tests__/ClassifyMessage/test1.json',
        content: JSON.stringify({ input: classifyMessageTest }),
      },
    ]
  },

  {
    id: 'chat-roles',
    name: 'Chat roles',
    description: 'Use a sequence of system and user messages when prompting',
    files: [
      {
        path: 'baml_src/main.baml',
        content: chatRolesBaml,
      },
      {
        path: 'baml_src/clients.baml',
        content: clientsBaml,
      },
      {
        path: 'baml_src/__tests__/ClassifyMessage/test1.json',
        content: JSON.stringify({ input: classifyMessageTest }),
      },
    ]
  },
  {
    id: 'symbol-tuning',
    name: 'Symbol tuning',
    description: 'Using "symbols" as aliases for terms in your prompt can improve your results',
    files: [
      {
        path: 'baml_src/main.baml',
        content: symbolTuningBaml,
      },
      {
        path: 'baml_src/clients.baml',
        content: clientsBaml,
      },
      {
        path: 'baml_src/__tests__/ClassifyMessage/test1.json',
        content: JSON.stringify({ input: classifyMessageTest }),
      },
    ]
  },
];
