import Link from 'next/link';

import { DocsShell } from '@/components/docs-shell';

export const metadata = {
  description: 'Build an end-to-end typed receipt extractor with BAML.',
  title: 'Build a structured extractor',
};

export default function StructuredExtractionTutorialPage() {
  return (
    <DocsShell
      breadcrumbs={[
        { href: '/tutorials', label: 'Tutorials' },
        { label: 'Structured extraction' },
      ]}
      description="Model a receipt, extract typed data, call the generated client, and strengthen the boundary."
      title="Build a structured extractor"
      toc={[
        { href: '#goal', label: 'What you will build' },
        { href: '#model', label: '1. Model the result' },
        { href: '#function', label: '2. Add the function' },
        { href: '#call', label: '3. Call the client' },
        { href: '#harden', label: '4. Harden the boundary' },
      ]}
    >
      <h2 id="goal">What you will build</h2>
      <p>
        This tutorial builds a server-side function that turns receipt text into
        a typed list of items and an optional total. It assumes a configured
        model client and a generated TypeScript client.
      </p>

      <h2 id="model">1. Model the result</h2>
      <p>
        Start with the data the application needs. Keep monetary fields numeric
        so callers do not need to parse display strings.
      </p>
      <pre>
        <code>{`class ReceiptItem {
  name string
  quantity int
  price float
}

class Receipt {
  items ReceiptItem[]
  total float?
}`}</code>
      </pre>

      <h2 id="function">2. Add the function</h2>
      <p>
        Declare the input and output, select the configured client, and include
        the generated output instructions in the prompt.
      </p>
      <pre>
        <code>{`function ExtractReceipt(text: string) -> Receipt {
  client GPT4o

  prompt #"
    Extract the receipt from the following text.

    {{ text }}

    {{ ctx.output_format }}
  "#
}`}</code>
      </pre>
      <blockquote>
        <p>
          Keep the prompt focused on the task. The return type already carries
          the structural contract into the generated output instructions.
        </p>
      </blockquote>

      <h2 id="call">3. Call the client</h2>
      <p>
        Check the BAML project, regenerate the client, and call the function
        from server-side application code.
      </p>
      <pre>
        <code>{`import { b } from "./baml_client"

export async function extractReceipt(text: string) {
  const receipt = await b.ExtractReceipt(text)

  return {
    lineItems: receipt.items.length,
    total: receipt.total ?? null,
  }
}`}</code>
      </pre>

      <h2 id="harden">4. Harden the boundary</h2>
      <ul>
        <li>
          Add representative tests for clean, noisy, and incomplete receipts.
        </li>
        <li>
          Decide whether a missing total is acceptable before making it
          optional.
        </li>
        <li>Keep credentials and model calls in server-only modules.</li>
        <li>Regenerate the client whenever the result model changes.</li>
      </ul>
      <p>
        Review the{' '}
        <Link href="/baml/bridges/typescript">TypeScript bridge</Link> for
        host-language details, or open the{' '}
        <Link href="/baml/book/foundations/functions">functions chapter</Link>{' '}
        for the underlying language model.
      </p>
    </DocsShell>
  );
}
