import Link from 'next/link';

import { DocsShell } from '@/components/docs-shell';

export const metadata = {
  description: 'Syntax and behavior reference for BAML function declarations.',
  title: 'Functions',
};

export default function FunctionsReferencePage() {
  return (
    <DocsShell
      breadcrumbs={[
        { href: '/baml', label: 'BAML' },
        { href: '/baml/language', label: 'Language reference' },
        { label: 'Functions' },
      ]}
      description="Declare named, typed operations with expression or model-backed bodies."
      title="Functions"
      toc={[
        { href: '#syntax', label: 'Syntax' },
        { href: '#parameters', label: 'Parameters' },
        { href: '#bodies', label: 'Function bodies' },
        { href: '#examples', label: 'Examples' },
        { href: '#related', label: 'Related pages' },
      ]}
    >
      <h2 id="syntax">Syntax</h2>
      <pre>
        <code>{`function Name(parameter: Type) -> ReturnType {
  // function body
}`}</code>
      </pre>
      <table>
        <thead>
          <tr>
            <th>Part</th>
            <th>Required</th>
            <th>Purpose</th>
          </tr>
        </thead>
        <tbody>
          <tr>
            <td>Function name</td>
            <td>Yes</td>
            <td>Names the generated client method.</td>
          </tr>
          <tr>
            <td>Parameters</td>
            <td>Yes</td>
            <td>Declares the named, typed inputs; the list may be empty.</td>
          </tr>
          <tr>
            <td>Return type</td>
            <td>Yes</td>
            <td>Defines the result expected by BAML and generated clients.</td>
          </tr>
          <tr>
            <td>Body</td>
            <td>Yes</td>
            <td>Computes the result or declares a client and prompt.</td>
          </tr>
        </tbody>
      </table>

      <h2 id="parameters">Parameters</h2>
      <p>
        Parameters are named and typed. Separate multiple parameters with
        commas. Callers pass them in declaration order through generated client
        methods.
      </p>
      <pre>
        <code>{`function Summarize(title: string, body: string, limit: int) -> string {
  // ...
}`}</code>
      </pre>

      <h2 id="bodies">Function bodies</h2>
      <p>
        Expression functions evaluate ordinary BAML expressions. Model-backed
        functions declare a client and prompt. Both forms must satisfy the
        declared return type.
      </p>
      <table>
        <thead>
          <tr>
            <th>Body kind</th>
            <th>Use it for</th>
          </tr>
        </thead>
        <tbody>
          <tr>
            <td>Expression</td>
            <td>Deterministic transforms, branching, and composition.</td>
          </tr>
          <tr>
            <td>Client + prompt</td>
            <td>Typed operations whose result is produced by a model.</td>
          </tr>
        </tbody>
      </table>

      <h2 id="examples">Examples</h2>
      <p>
        See a function in context in{' '}
        <Link href="/examples/classify-support-tickets">
          Classify support tickets
        </Link>
        . For a guided explanation, read the{' '}
        <Link href="/baml/book/foundations/functions">functions chapter</Link>.
      </p>

      <h2 id="related">Related pages</h2>
      <ul>
        <li>
          <Link href="/baml/bridges/typescript">TypeScript bridge</Link>
        </li>
        <li>
          <Link href="/tutorials/structured-extraction">
            Structured extraction tutorial
          </Link>
        </li>
      </ul>
    </DocsShell>
  );
}
