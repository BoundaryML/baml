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
    call: 'from baml_sdk import classify_ticket\n\nasync def handle(ticket):\n    label = await classify_ticket(ticket.body)\n    if label == "BUG":\n        escalate(ticket)',
    file: 'app/tickets.py',
    install: 'pip install baml_bridge',
    name: 'Python',
    target: 'python/pydantic',
  },
  {
    call: 'import { classify_ticket } from "./baml_sdk/index.js";\n\nexport async function handle(ticket: Ticket) {\n  const label = await classify_ticket(ticket.body);\n  if (label === "BUG") escalate(ticket);\n}',
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
    call: 'use baml_sdk::classify_ticket_sync;\n\nfn handle(ticket: &Ticket) -> anyhow::Result<()> {\n    let label = classify_ticket_sync(ticket.body.clone())?;\n    // ...\n    Ok(())\n}',
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
    call: 'import Baml\n\nfunc handle(_ ticket: Ticket) throws {\n    let label = try Baml.classify_ticket_sync(body: ticket.body)\n    // ...\n}',
    file: 'Tickets.swift',
    install: 'add the generated Baml package to your build',
    name: 'Swift',
    target: 'swift',
  },
];

// ---------- samples: the same route, shown with different kinds of BAML ----------

// Each sample is one BAML file and the call to it from every language, in the shapes the SDK tests use
// (sdk_tests/): Python and TypeScript keep the BAML names, Go goes PascalCase, Rust and Swift use the
// _sync form, Java goes through Fns.
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
    Label this support ticket.
    \${body}
  \`
}`,
    calls: {
      'C++':
        '#include <baml_sdk.h>\n\nauto label = baml_sdk::classify_ticket(ticket.body);\nif (label == baml_sdk::Label::BUG) escalate(ticket);',
      Go: 'label, err := baml_sdk.ClassifyTicket(ctx, t.Body)\nif err != nil {\n\treturn err\n}\nif label == baml_sdk.LabelBUG {\n\tescalate(t)\n}',
      Java: 'import baml_sdk.Fns;\n\nvar label = Fns.classify_ticket(ticket.body());\nif (label == Label.BUG) escalate(ticket);',
      Python:
        'from baml_sdk import classify_ticket, Label\n\nlabel = await classify_ticket(ticket.body)\nif label == Label.BUG:\n    escalate(ticket)',
      Rust: 'use baml_sdk::{classify_ticket_sync, Label};\n\nlet label =\n    classify_ticket_sync(ticket.body.clone())?;\nif label == Label::BUG {\n    escalate(&ticket);\n}',
      Swift:
        'import Baml\n\nlet label = try Baml.classify_ticket_sync(\n    body: ticket.body\n)\nif label == .BUG { escalate(ticket) }',
      TypeScript:
        'import { classify_ticket } from "./baml_sdk/index.js";\n\nconst label = await classify_ticket(ticket.body);\nif (label === "BUG") escalate(ticket);',
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
    Extract the invoice from this email.
    \${text}
  \`
}`,
    calls: {
      'C++':
        '#include <baml_sdk.h>\n\nauto invoice = baml_sdk::extract_invoice(email.body);\npay(invoice.vendor, invoice.total);',
      Go: 'invoice, err := baml_sdk.ExtractInvoice(\n\tctx, email.Body,\n)\nif err != nil {\n\treturn err\n}\npay(invoice.Vendor, invoice.Total)',
      Java: 'import baml_sdk.Fns;\n\nvar invoice = Fns.extract_invoice(email.body());\npay(invoice.vendor(), invoice.total());',
      Python:
        'from baml_sdk import extract_invoice\n\ninvoice = await extract_invoice(email.body)\n# a pydantic model: invoice.total is a float\npay(invoice.vendor, invoice.total)',
      Rust: 'use baml_sdk::extract_invoice_sync;\n\nlet invoice =\n    extract_invoice_sync(email.body.clone())?;\npay(&invoice.vendor, invoice.total);',
      Swift:
        'import Baml\n\nlet invoice = try Baml.extract_invoice_sync(\n    text: email.body\n)\npay(invoice.vendor, invoice.total)',
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
    Find the tickets that match: \${query}
  \`
}`,
    calls: {
      'C++':
        '#include <baml_sdk.h>\n\nauto page = baml_sdk::search_tickets("refund");\nfor (const auto& t : page.items) review(t.id);',
      Go: 'page, err := baml_sdk.SearchTickets(ctx, "refund")\nif err != nil {\n\treturn err\n}\nfor _, t := range page.Items {\n\treview(t.Id)\n}',
      Java: 'import baml_sdk.Fns;\n\nvar page = Fns.search_tickets("refund");\nfor (var t : page.items()) review(t.id());',
      Python:
        'from baml_sdk import search_tickets\n\npage = await search_tickets("refund")\n# Page[Ticket]: items are typed Tickets\nfor t in page.items:\n    review(t.id)',
      Rust: 'use baml_sdk::search_tickets_sync;\n\nlet page = search_tickets_sync("refund".into())?;\nfor t in &page.items {\n    review(&t.id);\n}',
      Swift:
        'import Baml\n\nlet page = try Baml.search_tickets_sync(\n    query: "refund"\n)\nfor t in page.items { review(t.id) }',
      TypeScript:
        'import { search_tickets } from "./baml_sdk/index.js";\n\nconst page = await search_tickets("refund");\nfor (const t of page.items) review(t.id);',
    },
    file: 'search.baml',
    name: 'Generics',
    note: 'A generic Page<T> in BAML stays generic in your language: Page<Ticket>, typed all the way down.',
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
    Label this support ticket.
    \${body}
  \`
}

// Your own code comes in as a function.
function triage(
  body: string,
  escalate: (string) -> string,
) -> string {
  if (classify_ticket(body) == Label.BUG) {
    return escalate(body);
  }
  "handled"
}`,
    calls: {
      'C++':
        '#include <baml_sdk.h>\n\nauto escalate = [](std::string body) {\n  return jira::create(body);\n};\nauto result =\n    baml_sdk::triage(ticket.body, escalate);',
      Go: 'escalate := func(\n\t_ context.Context, body string,\n) string {\n\treturn jira.Create(body)\n}\nresult, err := baml_sdk.Triage(\n\tctx, t.Body, escalate,\n)',
      Java: 'import baml_sdk.Fns;\n\nFunction<String, String> escalate =\n    body -> jira.create(body);\nvar result =\n    Fns.triage(ticket.body(), escalate);',
      Python:
        'from baml_sdk import triage\n\ndef escalate(body: str) -> str:\n    return jira.create(body)\n\nresult = await triage(\n    body=ticket.body, escalate=escalate\n)',
      Rust: 'use baml_sdk::triage_sync;\n\nlet result = triage_sync(\n    ticket.body.clone(),\n    |body: String| jira::create(&body),\n)?;',
      Swift:
        'import Baml\n\nlet result = try Baml.triage_sync(\n    body: ticket.body,\n    escalate: { body in jira.create(body) }\n)',
      TypeScript:
        'import { triage } from "./baml_sdk/index.js";\n\nconst result = await triage(ticket.body, (body) =>\n  jira.create(body),\n);',
    },
    file: 'triage.baml',
    name: 'Your functions',
    note: 'Pass a lambda in. BAML calls your code back, in your language.',
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
