import Link from 'next/link';

import { DocsShell } from '@/components/docs-shell';

export const metadata = {
  description: 'Learn BAML and explore its language and package reference.',
  title: 'BAML',
};

export default function BamlPage() {
  return (
    <DocsShell
      breadcrumbs={[{ label: 'BAML' }]}
      description="The language for building reliable, typed AI applications. Start quickly, learn systematically, or jump directly to reference material."
      title="BAML"
      toc={[
        { href: '#open-language', label: 'Open language' },
        { href: '#start', label: 'Start learning' },
        { href: '#reference', label: 'Reference' },
        { href: '#bridges', label: 'Language bridges' },
      ]}
    >
      <p>
        <strong>
          BAML is a language and toolchain for defining reliable, typed AI
          application interfaces.
        </strong>
      </p>
      <p>
        Define functions, prompts, and data models in BAML, then generate the
        host-language client used by your application. The language keeps the
        model-facing contract explicit while the generated client provides a
        typed application boundary.
      </p>
      <p>
        This documentation is organized around the language itself, practical
        workflows, generated standard-package references, and the CLI surface
        that ships with each toolchain release.
      </p>
      <h2 id="open-language">Open language</h2>
      <p>
        BAML source stays readable and portable. The language reference explains
        the syntax and semantics, while the generated package pages document the
        exact APIs exposed by a selected BAML release.
      </p>
      <ul>
        <li>
          <strong>Explicit contracts:</strong> Functions and types make the
          model boundary inspectable.
        </li>
        <li>
          <strong>Typed clients:</strong> Generated code carries those contracts
          into the host application.
        </li>
        <li>
          <strong>Release-specific reference:</strong> Package and CLI pages
          come from one selected compiled toolchain.
        </li>
      </ul>
      <h2 id="start">Start learning</h2>
      <p>
        Begin with <Link href="/baml/get-started">Get started</Link> for the
        shortest path to a checked BAML function. Continue through the{' '}
        <Link href="/baml/book">BAML book</Link> for a deliberate,
        chapter-by-chapter path through the language.
      </p>
      <h2 id="reference">Reference</h2>
      <p>
        Use the <Link href="/baml/language">language reference</Link> for
        concepts, syntax, types, and declarations. Browse{' '}
        <Link href="/baml/packages">standard packages</Link> for APIs generated
        from the exact selected BAML toolchain.
      </p>
      <h2 id="bridges">Language bridges</h2>
      <p>
        Understand how BAML values, errors, streaming, and generated names map
        into the host language used by your application in the{' '}
        <Link href="/baml/bridges">language bridge overview</Link>.
      </p>
    </DocsShell>
  );
}
