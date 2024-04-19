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

function stringSpanTest(functionName: string, testName: string): StringSpan {
  return {
    value: testName,
    start: 0,
    end: 0,
    source_file: `baml_src/__tests__/${functionName}/${testName}.json`,
  }
}

const extractNamesBaml = `function ExtractNames(input: string) -> string[] {
  client GPT4
  prompt #"
    Extract the names from this INPUT:
  
    INPUT:
    ---
    {{ input }}
    ---

    {{ ctx.output_format }}

    JSON array:
  "#
}
`;

const extractNamesTest = {
  "input": "\"Attention Is All You Need\" is a landmark[1][2] 2017 research paper by Google.[3] Authored by eight scientists, it was responsible for expanding 2014 attention mechanisms proposed by Bahdanau et. al. into a new deep learning architecture known as the transformer. The paper is considered by some to be a founding document for modern artificial intelligence, as transformers became the main architecture of large language models.[4][5] At the time, the focus of the research was on improving Seq2seq techniques for machine translation, but even in their paper the authors saw the potential for other tasks like question answering and for what is now called multimodal Generative AI.\n\nThe paper's title is a reference to the song \"All You Need Is Love\" by the Beatles.[6]\n\nAs of 2024, the paper has been cited more than 100,000 times.[7]"
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
  client GPT4

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
  client GPT4
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
    "body": "Content-Type: text/plain; charset=utf-8\nContent-Transfer-Encoding: 7bit\n\nAmazon Shipping Confirmation\nhttps://www.amazon.com?ie=UTF8&ref_=scr_home\n\n____________________________________________________________________\n\nHi Samuel, your package will arrive:\n\nThursday, April 4\n\nTrack your package:\nhttps://www.amazon.com/gp/your-account/ship-track?ie=UTF8&orderId=113-7540940-3785857&packageIndex=0&shipmentId=Gx7wk71F9&ref_=scr_pt_tp_t\n\n\n\nOn the way:\nWood Square Dowel Rods...\nOrder #113-7540940-3785857\n\n\n\nAn Amazon driver may contact you by text message or call you for help on the day of delivery.    \n\nShip to:\n    Sam\n    SEATTLE, WA\n\nShipment total:\n$0.00\nRewards points applied\n\n\nReturn or replace items in Your orders\nhttps://www.amazon.com/gp/css/order-history?ie=UTF8&ref_=scr_yo\n\nLearn how to recycle your packaging at Amazon Second Chance(https://www.amazon.com/amsc?ref_=ascyorn).\n\n",
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
  client GPT4

  prompt #"
    {# You can use _.chat("system") to start a system message #}
    {{ _.chat("system") }}

    Classify the following INPUT into ONE
    of the following categories:

    {{ ctx.output_format }}

    {# And _.chat("user") to start a user message #}
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
  client GPT4

  prompt #"
    Classify the following INPUT into ONE
    of the following categories:

    INPUT: {{ input }}

    {{ ctx.output_format }}

    Response:
  "#
}
`;

const clientsBaml = `client<llm> GPT4 {
  provider baml-openai-chat
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
    id: 'extract-names',
    name: 'ExtractNames',
    description: 'Extract names from a given input',
    files: [
      {
        path: 'baml_src/main.baml',
        content: extractNamesBaml,
      },
      {
        path: 'baml_src/clients.baml',
        content: clientsBaml,
      },
      {
        path: 'baml_src/__tests__/ExtractNames/test1.json',
        content: JSON.stringify({ input: extractNamesTest }),
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
