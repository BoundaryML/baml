import Link from 'next/link';

import { DocsShell } from '@/components/docs-shell';

export const metadata = {
  description: 'Go from installing BAML to checking your first function.',
  title: 'Get started',
};

export default function GetStartedPage() {
  return (
    <DocsShell
      breadcrumbs={[{ href: '/baml', label: 'BAML' }, { label: 'Get started' }]}
      description="The shortest path from installing BAML to checking your first function."
      title="Get started"
      toc={[
        { href: '#install', label: 'Install the toolchain' },
        { href: '#create', label: 'Create a project' },
        { href: '#check', label: 'Check your work' },
      ]}
    >
      <h2 id="install">Install the toolchain</h2>
      <p>
        Install the BAML CLI and editor support for your environment. The
        versioned CLI reference will remain the canonical source for exact
        installation methods and supported environments.
      </p>
      <h2 id="create">Create a project</h2>
      <p>
        A normal BAML project contains a <code>baml.toml</code> file and a{' '}
        <code>baml_src/</code> directory. Put your functions and types under
        that source directory so the ordinary toolchain can discover them.
      </p>
      <pre>
        <code>{`my-project/
├── baml.toml
└── baml_src/
    └── main.baml`}</code>
      </pre>
      <h2 id="check">Check your work</h2>
      <p>
        Use the CLI to check the project before generating or running client
        code. Continue to the <Link href="/cli">BAML CLI overview</Link> for the
        versioned command surface.
      </p>
    </DocsShell>
  );
}
