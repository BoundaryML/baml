import Link from 'next/link';

import { DocsShell } from '@/components/docs-shell';

export const metadata = {
  description: 'How BAML values and calls map into TypeScript.',
  title: 'TypeScript bridge',
};

export default function TypeScriptBridgePage() {
  return (
    <DocsShell
      breadcrumbs={[
        { href: '/baml', label: 'BAML' },
        { href: '/baml/bridges', label: 'Language bridges' },
        { label: 'TypeScript' },
      ]}
      description="Type mappings, generated client calls, streaming behavior, and TypeScript-specific boundaries."
      title="TypeScript bridge"
      toc={[
        { href: '#compatibility', label: 'Compatibility' },
        { href: '#types', label: 'Type mappings' },
        { href: '#client', label: 'Generated client' },
        { href: '#streaming', label: 'Streaming' },
        { href: '#gotchas', label: 'Gotchas' },
      ]}
    >
      <h2 id="compatibility">Compatibility</h2>
      <p>
        Generate the TypeScript client with the same BAML toolchain selected by
        the project. Application code imports generated symbols locally; the
        runtime package provides the supporting media and execution types.
      </p>
      <table>
        <thead>
          <tr>
            <th>Concern</th>
            <th>TypeScript behavior</th>
          </tr>
        </thead>
        <tbody>
          <tr>
            <td>Calls</td>
            <td>Generated methods are asynchronous and return promises.</td>
          </tr>
          <tr>
            <td>Names</td>
            <td>
              BAML function and type names remain visible in generated code.
            </td>
          </tr>
          <tr>
            <td>Regeneration</td>
            <td>
              Generated files are replaced; application edits belong elsewhere.
            </td>
          </tr>
        </tbody>
      </table>

      <h2 id="types">Type mappings</h2>
      <div className="overflow-x-auto">
        <table>
          <thead>
            <tr>
              <th>BAML</th>
              <th>TypeScript</th>
              <th>Notes</th>
            </tr>
          </thead>
          <tbody>
            <tr>
              <td>
                <code>string</code>
              </td>
              <td>
                <code>string</code>
              </td>
              <td>Text values map directly.</td>
            </tr>
            <tr>
              <td>
                <code>int</code> / <code>float</code>
              </td>
              <td>
                <code>number</code>
              </td>
              <td>Keep integer constraints in mind at runtime boundaries.</td>
            </tr>
            <tr>
              <td>
                <code>T[]</code>
              </td>
              <td>
                <code>T[]</code>
              </td>
              <td>Element types remain explicit.</td>
            </tr>
            <tr>
              <td>
                <code>T?</code>
              </td>
              <td>Nullable generated type</td>
              <td>Do not use truthiness when zero or empty text is valid.</td>
            </tr>
            <tr>
              <td>
                <code>class</code>
              </td>
              <td>Generated object type</td>
              <td>Fields preserve their declared names and types.</td>
            </tr>
            <tr>
              <td>
                <code>enum</code>
              </td>
              <td>Generated enum</td>
              <td>
                Import the generated symbol instead of duplicating literals.
              </td>
            </tr>
          </tbody>
        </table>
      </div>

      <h2 id="client">Generated client</h2>
      <p>
        Import <code>b</code> from the generated client directory and await the
        method matching the BAML function name.
      </p>
      <pre>
        <code>{`import { b } from "./baml_client"
import type { Category } from "./baml_client/types"

export async function classify(input: string): Promise<Category> {
  return b.ClassifyMessage(input)
}`}</code>
      </pre>

      <h2 id="streaming">Streaming</h2>
      <p>
        Streaming calls live under <code>b.stream</code>. The generated stream
        API exposes incremental values and a final typed result; use the adapter
        appropriate to the application framework at its outer boundary.
      </p>
      <pre>
        <code>{`const stream = b.stream.ClassifyMessage(input)
const response = stream.toStreamable()`}</code>
      </pre>

      <h2 id="gotchas">Gotchas</h2>
      <ul>
        <li>Never edit generated client files by hand.</li>
        <li>
          Regenerate after changing a BAML function, class, enum, or generator
          configuration.
        </li>
        <li>
          Keep secret-bearing client construction on the server side of web
          applications.
        </li>
        <li>
          Treat partial streaming values separately from the final result type.
        </li>
      </ul>
      <p>
        Continue with the{' '}
        <Link href="/baml/language/functions">function reference</Link> to
        review the source declaration behind generated calls.
      </p>
    </DocsShell>
  );
}
