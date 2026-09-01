import Link from 'next/link';

const sections = [
  {
    href: '/baml',
    title: 'BAML',
    description: 'The language, book, standard library, and language bridges.',
  },
  {
    href: '/cli',
    title: 'CLI',
    description: 'Installation, commands, configuration, and releases.',
  },
  {
    href: '/bws',
    title: 'BWS',
    description: 'Deploy, observe, and debug BAML workloads.',
  },
  {
    href: '/tutorials',
    title: 'Tutorials',
    description: 'Learn complete workflows across BAML, the CLI, and BWS.',
  },
  {
    href: '/examples',
    title: 'Examples',
    description: 'Start from complete, runnable projects.',
  },
];

export default function HomePage() {
  return (
    <main className="mx-auto flex w-full max-w-6xl flex-1 flex-col px-6 py-20">
      <div className="max-w-3xl">
        <p className="mb-4 text-sm font-medium uppercase tracking-[0.2em] text-fd-muted-foreground">
          Boundary Developer
        </p>
        <h1 className="text-4xl font-semibold tracking-tight sm:text-6xl">
          Build reliable AI systems with BAML.
        </h1>
        <p className="mt-6 text-lg leading-8 text-fd-muted-foreground">
          Learn the language, use the CLI, ship with BWS, and run examples in
          the browser.
        </p>
      </div>

      <div className="mt-14 grid gap-4 md:grid-cols-2 lg:grid-cols-3">
        {sections.map((section) => (
          <Link
            key={section.href}
            href={section.href}
            className="rounded-xl border bg-fd-card p-6 transition-colors hover:bg-fd-accent"
          >
            <h2 className="text-xl font-semibold">{section.title}</h2>
            <p className="mt-2 leading-7 text-fd-muted-foreground">
              {section.description}
            </p>
          </Link>
        ))}
      </div>
    </main>
  );
}
