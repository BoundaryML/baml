import Link from 'next/link';

import { DocsShell } from '@/components/docs-shell';

export const metadata = {
  description: 'A compact enum-returning support ticket classifier.',
  title: 'Classify support tickets',
};

export default function ClassifySupportTicketsExamplePage() {
  return (
    <DocsShell
      breadcrumbs={[
        { href: '/examples', label: 'Examples' },
        { label: 'Classify support tickets' },
      ]}
      description="A focused BAML example that maps an incoming message to one known support category."
      title="Classify support tickets"
      toc={[
        { href: '#source', label: 'Source' },
        { href: '#call', label: 'Call it' },
        { href: '#why', label: 'Why this shape' },
      ]}
    >
      <h2 id="source">Source</h2>
      <pre>
        <code>{`enum Category {
  Refund
  CancelOrder
  TechnicalSupport
  AccountIssue
  Question
}

function ClassifyMessage(input: string) -> Category {
  client GPT4

  prompt #"
    Classify the following message into one category:

    {{ input }}

    {{ ctx.output_format }}
  "#
}

test ClassifiesAccountProblem {
  functions [ClassifyMessage]
  args {
    input "I cannot sign in to my account"
  }
}`}</code>
      </pre>

      <h2 id="call">Call it</h2>
      <pre>
        <code>{`import { b } from "./baml_client"

const category = await b.ClassifyMessage(
  "I was charged twice for my order",
)`}</code>
      </pre>

      <h2 id="why">Why this shape</h2>
      <p>
        An enum makes the output space explicit and gives generated clients a
        named result type. The application can switch on known values instead of
        normalizing arbitrary model text.
      </p>
      <p>
        Read the <Link href="/baml/language/functions">function reference</Link>{' '}
        for syntax details or expand the same pattern in the{' '}
        <Link href="/tutorials/structured-extraction">
          structured extraction tutorial
        </Link>
        .
      </p>
    </DocsShell>
  );
}
