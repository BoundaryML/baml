// The adopt lab's code samples, shared by the page (which highlights them on the server) and the
// lab (which shows them).

// How each language's generated SDK is produced and called, following the SDK tests in sdk_tests/.
export const LANGS: {
  call: string;
  file: string;
  install: string;
  name: string;
  target: string;
}[] = [
  {
    call: 'from baml_sdk import classify_ticket\n\nasync def handle(ticket):\n    label = await classify_ticket(ticket.body)\n    # ...',
    file: 'app/tickets.py',
    install: 'pip install baml_bridge',
    name: 'Python',
    target: 'python/pydantic',
  },
  {
    call: 'import { classify_ticket } from "./baml_sdk/index.js";\n\nexport async function handle(ticket: Ticket) {\n  const label = await classify_ticket(ticket.body);\n  // ...\n}',
    file: 'src/tickets.ts',
    install: 'npm install @boundaryml/baml-bridge',
    name: 'TypeScript',
    target: 'typescript/node',
  },
  {
    call: 'import "example.com/app/baml_sdk"\n\nfunc Handle(ctx context.Context, t Ticket) error {\n\tlabel, err := baml_sdk.ClassifyTicket(ctx, t.Body)\n\tif err != nil {\n\t\treturn err\n\t}\n\t// ...\n}',
    file: 'tickets/handle.go',
    install: 'the runtime downloads on first use',
    name: 'Go',
    target: 'go',
  },
  {
    call: 'use baml_sdk::classify_ticket;\n\nasync fn handle(ticket: &Ticket) -> anyhow::Result<()> {\n    let label = classify_ticket(ticket.body.clone()).await?;\n    // ...\n    Ok(())\n}',
    file: 'src/tickets.rs',
    install: 'the runtime downloads on first use',
    name: 'Rust',
    target: 'rust',
  },
  {
    call: 'import baml_sdk.Fns;\n\nvoid handle(Ticket ticket) {\n    var label = Fns.classify_ticket(ticket.body());\n    // ...\n}',
    file: 'Tickets.java',
    install: 'add the generated baml_sdk to your build',
    name: 'Java',
    target: 'java',
  },
  {
    call: '#include <baml_sdk.h>\n\nvoid handle(const Ticket& ticket) {\n  auto label = baml_sdk::classify_ticket(ticket.body);\n  // ...\n}',
    file: 'tickets.cc',
    install: 'add the generated baml_sdk to your build',
    name: 'C++',
    target: 'cpp',
  },
  {
    call: 'import Baml\n\nfunc handle(_ ticket: Ticket) async throws {\n    let label = try await Baml.classify_ticket(body: ticket.body)\n    // ...\n}',
    file: 'Tickets.swift',
    install: 'add the generated Baml package to your build',
    name: 'Swift',
    target: 'swift',
  },
];

// ---------- samples: the same route, shown with different kinds of BAML ----------

// Each sample is one BAML file and the call to it from every language, in the shapes the SDK tests use
// (sdk_tests/): Python and TypeScript keep the BAML names, Go goes PascalCase, Rust and Swift await the
// async form, Java goes through Fns.
export const SAMPLES: {
  baml: string;
  calls: Record<string, string>;
  file: string;
  name: string;
  note: string;
}[] = [
  {
    baml: `enum Label {
  BILLING
  BUG
  OTHER
}

function classify_ticket(body: string) -> Label {
  client: "openai/gpt-5-mini"
  prompt: \`
    \${role("user")}
    Label this support ticket.
    \${body}

    \${ctx.output_format()}
  \`
}

function triage(
  body: string,
  // Your own code comes in as a function.
  escalate: (string) -> string,
) -> string {
  match (classify_ticket(body)) {
    Label.BUG => escalate(body),
    Label.BILLING => "sent to billing",
    Label.OTHER => "answered from the docs",
  }
}`,
    calls: {
      'C++':
        '#include <baml_sdk.h>\n#include <linear/client.h>\n\nlinear::Client client(\n    std::getenv("LINEAR_API_KEY"));\n\n// A regular C++ function.\nstd::string escalate(std::string body) {\n  auto issue = client.create_issue({\n      .team_id = TEAM_ID,\n      .title = body,\n  });\n  return issue.url;\n}\n\nauto result =\n    baml_sdk::triage(ticket.body, escalate);',
      Go: 'import "example.com/linear"\n\nvar client = linear.NewClient(\n\tos.Getenv("LINEAR_API_KEY"),\n)\n\n// A regular Go function.\nfunc escalate(\n\tctx context.Context, body string,\n) (string, error) {\n\tissue, err := client.Issues.Create(\n\t\tctx, linear.IssueCreateInput{\n\t\t\tTeamID: teamID,\n\t\t\tTitle:  body,\n\t\t},\n\t)\n\tif err != nil {\n\t\treturn "", err\n\t}\n\treturn issue.URL, nil\n}\n\nresult, err :=\n\tbaml_sdk.Triage(ctx, t.Body, escalate)',
      Java: 'import baml_sdk.Fns;\nimport com.example.linear.LinearClient;\n\nstatic final LinearClient linear =\n    LinearClient.fromEnv(); // LINEAR_API_KEY\n\n// A regular Java method.\nstatic CompletableFuture<String> escalate(\n    String body\n) {\n    return linear.issues()\n        .createAsync(TEAM_ID, body)\n        .thenApply(issue -> issue.url());\n}\n\nvar result =\n    Fns.triage(ticket.body(), Tickets::escalate);',
      Python:
        'import os\nfrom baml_sdk import triage\nfrom linear import AsyncLinear\n\nlinear = AsyncLinear(\n    api_key=os.environ["LINEAR_API_KEY"],\n)\n\n# A regular Python function.\nasync def escalate(body: str) -> str:\n    issue = await linear.issues.create(\n        team_id=TEAM_ID,\n        title=body,\n    )\n    return issue.url\n\nresult = await triage(\n    body=ticket.body, escalate=escalate\n)',
      Rust: 'use baml_sdk::triage;\nuse linear::{Client, Error, IssueCreateInput};\n\n// A regular Rust function.\nasync fn escalate(\n    body: String,\n) -> Result<String, Error> {\n    // Reads LINEAR_API_KEY.\n    let client = Client::from_env()?;\n    let issue = client\n        .create_issue(IssueCreateInput {\n            team_id: TEAM_ID.into(),\n            title: body,\n            ..Default::default()\n        })\n        .await?;\n    Ok(issue.url)\n}\n\nlet result =\n    triage(ticket.body.clone(), escalate).await?;',
      Swift:
        'import Baml\nimport Linear\n\nlet linear = LinearClient(\n    apiKey: ProcessInfo.processInfo\n        .environment["LINEAR_API_KEY"]!\n)\n\n// A regular Swift function.\nfunc escalate(\n    _ body: String\n) async throws -> String {\n    let issue = try await linear.createIssue(\n        teamId: teamId,\n        title: body\n    )\n    return issue.url\n}\n\nlet result = try await Baml.triage(\n    body: ticket.body,\n    escalate: escalate\n)',
      TypeScript:
        'import { LinearClient } from "@linear/sdk";\nimport { triage } from "./baml_sdk/index.js";\n\nconst linear = new LinearClient({\n  apiKey: process.env.LINEAR_API_KEY,\n});\n\n// A regular TypeScript function.\nasync function escalate(\n  body: string,\n): Promise<string> {\n  const created = await linear.createIssue({\n    teamId: TEAM_ID,\n    title: body,\n  });\n  const issue = await created.issue;\n  return issue?.url ?? "";\n}\n\nconst result =\n  await triage(ticket.body, escalate);',
    },
    file: 'triage.baml',
    name: 'Your functions',
    note: 'Pass a lambda in. BAML calls your code back, in your language.',
  },
  {
    baml: `enum Label {
  BILLING
  BUG
  OTHER
}

function classify_ticket(body: string) -> Label {
  client: "openai/gpt-5-mini"
  prompt: \`
    \${role("user")}
    Label this support ticket.
    \${body}

    \${ctx.output_format()}
  \`
}`,
    calls: {
      'C++':
        '#include <baml_sdk.h>\n\nusing baml_sdk::Label;\nswitch (baml_sdk::classify_ticket(ticket.body)) {\n  case Label::BUG: escalate(ticket); break;\n  case Label::BILLING: send_to_billing(ticket); break;\n  case Label::OTHER: answer_from_docs(ticket); break;\n}',
      Go: 'label, err := baml_sdk.ClassifyTicket(ctx, t.Body)\nif err != nil {\n\treturn err\n}\nswitch label {\ncase baml_sdk.LabelBUG:\n\tescalate(t)\ncase baml_sdk.LabelBILLING:\n\tsendToBilling(t)\ncase baml_sdk.LabelOTHER:\n\tanswerFromDocs(t)\n}',
      Java: 'import baml_sdk.Fns;\n\nswitch (Fns.classify_ticket(ticket.body())) {\n    case BUG -> escalate(ticket);\n    case BILLING -> sendToBilling(ticket);\n    case OTHER -> answerFromDocs(ticket);\n}',
      Python:
        'from baml_sdk import classify_ticket, Label\n\nmatch await classify_ticket(ticket.body):\n    case Label.BUG:\n        escalate(ticket)\n    case Label.BILLING:\n        send_to_billing(ticket)\n    case Label.OTHER:\n        answer_from_docs(ticket)',
      Rust: 'use baml_sdk::{classify_ticket, Label};\n\nmatch classify_ticket(ticket.body.clone()).await? {\n    Label::BUG => escalate(&ticket),\n    Label::BILLING => send_to_billing(&ticket),\n    Label::OTHER => answer_from_docs(&ticket),\n}',
      Swift:
        'import Baml\n\nswitch try await Baml.classify_ticket(body: ticket.body) {\ncase .BUG: escalate(ticket)\ncase .BILLING: sendToBilling(ticket)\ncase .OTHER: answerFromDocs(ticket)\n}',
      TypeScript:
        'import { classify_ticket } from "./baml_sdk/index.js";\n\nswitch (await classify_ticket(ticket.body)) {\n  case "BUG": escalate(ticket); break;\n  case "BILLING": sendToBilling(ticket); break;\n  case "OTHER": answerFromDocs(ticket); break;\n}',
    },
    file: 'triage.baml',
    name: 'An enum',
    note: 'A model call that returns one of three labels, as your language’s own enum.',
  },
  {
    baml: `class Invoice {
  vendor string
  total float
  due_date string?
}

function extract_invoice(text: string) -> Invoice {
  client: "openai/gpt-5-mini"
  prompt: \`
    \${role("user")}
    Extract the invoice from this email.
    \${text}

    \${ctx.output_format()}
  \`
}`,
    calls: {
      'C++':
        '#include <baml_sdk.h>\n\nauto invoice = baml_sdk::extract_invoice(email.body);\npay(invoice.vendor, invoice.total);',
      Go: 'invoice, err := baml_sdk.ExtractInvoice(\n\tctx, email.Body,\n)\nif err != nil {\n\treturn err\n}\npay(invoice.Vendor, invoice.Total)',
      Java: 'import baml_sdk.Fns;\n\nvar invoice = Fns.extract_invoice(email.body());\npay(invoice.vendor(), invoice.total());',
      Python:
        'from baml_sdk import extract_invoice\n\ninvoice = await extract_invoice(email.body)\n# a pydantic model: invoice.total is a float\npay(invoice.vendor, invoice.total)',
      Rust: 'use baml_sdk::extract_invoice;\n\nlet invoice = extract_invoice(email.body.clone()).await?;\npay(&invoice.vendor, invoice.total);',
      Swift:
        'import Baml\n\nlet invoice = try await Baml.extract_invoice(\n    text: email.body\n)\npay(invoice.vendor, invoice.total)',
      TypeScript:
        'import { extract_invoice } from "./baml_sdk/index.js";\n\nconst invoice = await extract_invoice(email.body);\npay(invoice.vendor, invoice.total);',
    },
    file: 'invoices.baml',
    name: 'A class',
    note: 'Structured output as a typed object: a pydantic model, a TypeScript type, a Go struct.',
  },
  {
    baml: `class Ticket {
  id string
  body string
}

class Page<T> {
  items T[]
  next_cursor string?
}

function search_tickets(
  query: string,
) -> Page<Ticket> {
  client: "openai/gpt-5-mini"
  prompt: \`
    \${role("user")}
    Find the tickets that match: \${query}

    \${ctx.output_format()}
  \`
}`,
    calls: {
      'C++':
        '#include <baml_sdk.h>\n\nauto page = baml_sdk::search_tickets("refund");\nfor (const auto& t : page.items) review(t.id);',
      Go: 'page, err := baml_sdk.SearchTickets(ctx, "refund")\nif err != nil {\n\treturn err\n}\nfor _, t := range page.Items {\n\treview(t.Id)\n}',
      Java: 'import baml_sdk.Fns;\n\nvar page = Fns.search_tickets("refund");\nfor (var t : page.items()) review(t.id());',
      Python:
        'from baml_sdk import search_tickets\n\npage = await search_tickets("refund")\n# Page[Ticket]: items are typed Tickets\nfor t in page.items:\n    review(t.id)',
      Rust: 'use baml_sdk::search_tickets;\n\nlet page = search_tickets("refund".into()).await?;\nfor t in &page.items {\n    review(&t.id);\n}',
      Swift:
        'import Baml\n\nlet page = try await Baml.search_tickets(\n    query: "refund"\n)\nfor t in page.items { review(t.id) }',
      TypeScript:
        'import { search_tickets } from "./baml_sdk/index.js";\n\nconst page = await search_tickets("refund");\nfor (const t of page.items) review(t.id);',
    },
    file: 'search.baml',
    name: 'Generics',
    note: 'A generic Page<T> in BAML stays generic in your language: Page<Ticket>, typed all the way down.',
  },
];

// Shiki language ids for each language's samples.
export const SHIKI: Record<string, string> = {
  'C++': 'cpp',
  Go: 'go',
  Java: 'java',
  Python: 'python',
  Rust: 'rust',
  Swift: 'swift',
  TypeScript: 'typescript',
};
