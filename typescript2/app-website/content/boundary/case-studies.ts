import { byNewest, type CaseStudy } from './types';

// Three example case studies. Every company, number and quote here is a placeholder: the names are
// the same fictional ones already used on the homepage (Ledgerline, Kestrel Labs, Sundial), and each
// entry carries `placeholder: true` so it's easy to find before launch (the flag isn't shown on the
// page). There are no results or quote yet: add real ones (both optional) once the customer signs off.
//
// The prose describes only things the product actually does (tracing with no SDK, SQL over every
// call, boards, profiling). Replace the subject; the structure can stay.

const CASE_STUDIES: CaseStudy[] = [
  {
    cardZoom: 2.2,
    company: 'Ledgerline',
    crop: '18% 6%',
    date: '2026-10-02',
    dek: 'How a payables team moved invoice extraction into BAML and stopped guessing which model ran.',
    facts: [
      { term: 'Industry', value: 'Fintech' },
      { term: 'Stack', value: 'Python, BAML' },
      { term: 'Using', value: 'Profiling, SQL, boards' },
      { term: 'Key function', value: 'ExtractInvoice' },
    ],
    painting: '/boundary/painting-hills.jpg',
    placeholder: true,
    sections: [
      {
        body: [
          {
            text: 'Ledgerline reads supplier invoices and turns them into ledger entries. The extraction prompt started as a string in one Python service, then got copied into a batch job and into a reconciliation script. The three copies drifted. When a European invoice came back with the VAT line in the wrong field, nobody could say which copy had produced it, which model it had used, or whether the same thing had happened the week before.',
            type: 'paragraph',
          },
          {
            text: "The team's answer to that had been more logging, shipped after the fact, which only helped if the problem happened again.",
            type: 'paragraph',
          },
        ],
        heading: 'The problem',
        id: 'the-problem',
      },
      {
        body: [
          {
            text: 'They moved the extractor into a single BAML function with a typed return, and deleted the other two copies. The callers import a generated client, so the field names are checked at build time rather than discovered in a stack trace.',
            type: 'paragraph',
          },
          {
            code: `class Invoice {
  vendor: string,
  total: float,
  due_date: string?,
}

function ExtractInvoice(file: pdf) -> Invoice {
  client: "openai/gpt-4o-mini"
  prompt: \`
    Pull the fields out of \${file}.
    \${ctx.output_format()}
  \`
}`,
            filename: 'extract.baml',
            lang: 'baml',
            type: 'code',
          },
          {
            text: 'Tests sit next to the function. `baml test ExtractInvoice` runs against invoices captured from production, so a change to the prompt is checked against the documents that actually caused trouble.',
            type: 'paragraph',
          },
        ],
        heading: 'What they built with BAML',
        id: 'what-they-built-with-baml',
      },
      {
        body: [
          {
            text: 'Because the function is written in BAML, every call is profiled with no SDK to wire up and no spans to add. Inputs and outputs are captured for the functions the team opted in, and the rest stay closed.',
            type: 'paragraph',
          },
          {
            text: 'When an invoice looks wrong they query what already happened instead of adding logging and waiting:',
            type: 'paragraph',
          },
          {
            code: `SELECT model, count(*) AS calls, avg(cost_usd), avg(accuracy)
FROM calls_v1
WHERE fn = 'ExtractInvoice'
GROUP BY model`,
            filename: 'baml query',
            lang: 'sql',
            type: 'code',
          },
          {
            text: 'A Studio board keeps the same query on screen for the whole team, so the number in the standup and the number in the query are the same number.',
            type: 'paragraph',
          },
        ],
        heading: 'How Boundary fits',
        id: 'how-boundary-fits',
      },
    ],
    slug: 'ledgerline',
    tags: ['Fintech', 'Document extraction'],
    title:
      'One invoice extractor, five currencies, no copy of the prompt anywhere else',
  },

  {
    cardZoom: 2.4,
    company: 'Kestrel Labs',
    crop: '50% 62%',
    date: '2026-09-18',
    dek: 'How a platform team replaced a hand-rolled eval notebook with one table and one query.',
    facts: [
      { term: 'Industry', value: 'Developer tools' },
      { term: 'Stack', value: 'TypeScript, Python, BAML' },
      { term: 'Using', value: 'Profiling, SQL, boards' },
      { term: 'Key function', value: 'ClassifyTicket' },
    ],
    painting: '/boundary/painting-hills.jpg',
    placeholder: true,
    sections: [
      {
        body: [
          {
            text: 'Kestrel Labs routes inbound support tickets with a model. Two services call the classifier, one in TypeScript and one in Python, and until recently each had its own prompt and its own idea of what the categories were.',
            type: 'paragraph',
          },
          {
            text: 'Measuring it meant a weekly notebook: a CSV from the eval tool, traces from the observability tool, and a join on an ID that only mostly matched. The number took a day to produce and nobody defended it very hard.',
            type: 'paragraph',
          },
        ],
        heading: 'The problem',
        id: 'the-problem',
      },
      {
        body: [
          {
            text: 'One BAML function now owns the categories, and both services call a generated client. Both are reading the same type, so a new category is a single edit rather than a coordinated release.',
            type: 'paragraph',
          },
          {
            code: `class Ticket {
  category: string,
  urgency: int,
  summary: string?,
}

function ClassifyTicket(body: string) -> Ticket {
  client: "openai/gpt-4o-mini"
  prompt: \`
    Classify the support ticket below.
    \${body}
    \${ctx.output_format()}
  \`
}`,
            filename: 'tickets.baml',
            lang: 'baml',
            type: 'code',
          },
        ],
        heading: 'What they built with BAML',
        id: 'what-they-built-with-baml',
      },
      {
        body: [
          {
            text: 'Every call to `ClassifyTicket` is profiled, from both services, with the same function name on both. The weekly notebook became a query over `calls_v1` that anyone on the team can read and change.',
            type: 'paragraph',
          },
          {
            text: 'The same data backs the boards in Studio, so the support leads look at a dashboard and the platform engineers look at the SQL underneath it, and the two agree by construction.',
            type: 'paragraph',
          },
        ],
        heading: 'How Boundary fits',
        id: 'how-boundary-fits',
      },
    ],
    slug: 'kestrel-labs',
    tags: ['Developer tools', 'Classification'],
    title: 'Ticket classification the support team can check for itself',
  },

  {
    cardZoom: 2.6,
    company: 'Sundial',
    crop: '88% 96%',
    date: '2026-08-28',
    dek: 'How a product team swapped the model behind their summaries and could say afterwards what it cost them.',
    facts: [
      { term: 'Industry', value: 'B2B SaaS' },
      { term: 'Stack', value: 'TypeScript, BAML' },
      { term: 'Using', value: 'Profiling, SQL, boards' },
      { term: 'Key function', value: 'SummarizeCall' },
    ],
    painting: '/boundary/painting-hills.jpg',
    placeholder: true,
    sections: [
      {
        body: [
          {
            text: 'Sundial summarizes sales calls inside its own product. The summaries were expensive, and the obvious saving was a smaller model, but the team had no way to show what that would do to quality beyond reading a dozen outputs and forming an opinion.',
            type: 'paragraph',
          },
          {
            text: "Opinions don't survive a customer escalation, so the change kept getting postponed.",
            type: 'paragraph',
          },
        ],
        heading: 'The problem',
        id: 'the-problem',
      },
      {
        body: [
          {
            text: 'The summarizer moved into BAML with a typed return, which made the model a field rather than a decision baked into application code.',
            type: 'paragraph',
          },
          {
            code: `class CallSummary {
  headline: string,
  next_step: string?,
  sentiment: string,
}

function SummarizeCall(transcript: string) -> CallSummary {
  client: "openai/gpt-4o-mini"
  prompt: \`
    Summarize the sales call below for the account owner.
    \${transcript}
    \${ctx.output_format()}
  \`
}`,
            filename: 'calls.baml',
            lang: 'baml',
            type: 'code',
          },
          {
            text: "Changing the client is one line, and the callers don't know it happened.",
            type: 'paragraph',
          },
        ],
        heading: 'What they built with BAML',
        id: 'what-they-built-with-baml',
      },
      {
        body: [
          {
            text: 'Both models ran against real traffic, and every call landed in the same table with the same function name. The comparison was a `GROUP BY model` rather than a notebook, and it answered the cost question and the quality question from one row set.',
            type: 'paragraph',
          },
          {
            text: 'Sundial keeps its data in its own AWS account. Private values never leave their data plane, and the team still queries it from Studio, the CLI and curl.',
            type: 'paragraph',
          },
        ],
        heading: 'How Boundary fits',
        id: 'how-boundary-fits',
      },
    ],
    slug: 'sundial',
    tags: ['B2B SaaS', 'Summarization'],
    title: 'Call summaries that kept working after the model changed',
  },
];

/** Every case study, newest first. */
export function getAllCaseStudies(): CaseStudy[] {
  return [...CASE_STUDIES].sort(byNewest);
}

/** One case study by slug, or undefined. */
export function getCaseStudy(slug: string): CaseStudy | undefined {
  return CASE_STUDIES.find((c) => c.slug === slug);
}

/** The next case study to read, wrapping round at the end. Undefined when there's only one. */
export function getNextCaseStudy(slug: string): CaseStudy | undefined {
  const all = getAllCaseStudies();
  if (all.length < 2) return undefined;
  const i = all.findIndex((c) => c.slug === slug);
  return i === -1 ? undefined : all[(i + 1) % all.length];
}

/** The study the homepage links to from "Read the case study". */
export function getFeaturedCaseStudy(): CaseStudy {
  return getAllCaseStudies()[0];
}
