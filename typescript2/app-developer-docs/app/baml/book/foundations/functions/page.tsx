import Link from 'next/link';

import { BamlSnippet } from '@/components/baml-snippet';
import { DocsShell } from '@/components/docs-shell';

export const metadata = {
  description: 'Define the typed operations at the center of a BAML project.',
  title: 'Functions',
};

export default function FunctionsChapterPage() {
  return (
    <DocsShell
      breadcrumbs={[
        { href: '/baml', label: 'BAML' },
        { href: '/baml/book', label: 'Book' },
        { href: '/baml/book/foundations', label: 'Foundations' },
        { label: 'Functions' },
      ]}
      description="Chapter 1 · Define a typed operation, connect it to a model, and call it from application code."
      title="Functions"
      toc={[
        { href: '#anatomy', label: 'Anatomy of a function' },
        { href: '#typed-boundary', label: 'The typed boundary' },
        { href: '#model-backed', label: 'Model-backed functions' },
        { href: '#application', label: 'Call from an application' },
        { href: '#next', label: 'Where to go next' },
      ]}
    >
      <p>
        Functions are the main entry points into a BAML program. Every function
        declares a name, a parameter list, a return type, and a body. The body
        can compute a value directly or use a client and prompt to ask a model
        for the declared result.
      </p>

      <h2 id="anatomy">Anatomy of a function</h2>
      <p>The smallest useful function returns a typed value directly.</p>
      <BamlSnippet id="functions/return-number" />
      <p>
        <code>ReturnNumber</code> is the function name, <code>value</code> is a
        named parameter, and the arrow introduces the return type. The final
        expression in the body becomes the result.
      </p>

      <h2 id="typed-boundary">The typed boundary</h2>
      <p>
        Inputs and outputs can use primitives, optional values, lists, enums,
        classes, and other named types. Prefer a named class when downstream
        code cares about more than one field.
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
      <blockquote>
        <p>
          Optionality is part of the contract. Use <code>float?</code> only when
          the application can genuinely handle a missing value.
        </p>
      </blockquote>
      <p>
        The compiler rejects a body whose value does not match the declared
        return type. This deliberately invalid example is checked for the
        expected <code>E0001</code> diagnostic during documentation validation.
      </p>
      <BamlSnippet id="errors/invalid-return-type" />

      <h2 id="model-backed">Model-backed functions</h2>
      <p>
        A model-backed function selects a configured client and supplies a
        prompt. Including <code>{'{{ ctx.output_format }}'}</code> gives the
        model the output instructions derived from the return type.
      </p>
      <pre>
        <code>{`function ExtractReceipt(text: string) -> Receipt {
  client GPT4o

  prompt #"
    Extract the receipt from this input:

    {{ text }}

    {{ ctx.output_format }}
  "#
}`}</code>
      </pre>

      <h2 id="application">Call from an application</h2>
      <p>
        After generation, the TypeScript client exposes the function under the
        same name. Its promise resolves to the generated <code>Receipt</code>
        type.
      </p>
      <pre>
        <code>{`import { b } from "./baml_client"

const receipt = await b.ExtractReceipt(input)
console.log(receipt.items)`}</code>
      </pre>

      <h2 id="next">Where to go next</h2>
      <p>
        Use the <Link href="/baml/language/functions">function reference</Link>{' '}
        for a condensed syntax view, or build a complete flow in the{' '}
        <Link href="/tutorials/structured-extraction">
          structured extraction tutorial
        </Link>
        .
      </p>
    </DocsShell>
  );
}
