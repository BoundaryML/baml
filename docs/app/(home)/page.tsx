import { ArrowRight } from 'lucide-react';
import Link from 'next/link';

const sections = [
  {
    href: '/baml',
    title: 'BAML',
    description: 'Language concepts, the book, and generated standard library reference.',
  },
  {
    href: '/cli',
    title: 'CLI',
    description: 'Installation, workflows, and the complete generated command reference.',
  },
  {
    href: '/bws',
    title: 'Boundary Web Services',
    description: 'Documentation is coming soon.',
  },
] as const;

export default function HomePage() {
  return (
    <main className="shadcn-home">
      <div className="shadcn-home__intro">
        <h1>Boundary Developer</h1>
        <p>Documentation for building reliable AI software with BAML.</p>
        <div className="shadcn-home__actions">
          <Link href="/baml">
            Get started <ArrowRight aria-hidden="true" />
          </Link>
          <Link href="/baml/book">Read the book</Link>
        </div>
      </div>

      <div className="shadcn-home__grid">
        {sections.map((section) => (
          <Link key={section.href} href={section.href}>
            <h2>{section.title}</h2>
            <p>{section.description}</p>
            <span>Open documentation <ArrowRight aria-hidden="true" /></span>
          </Link>
        ))}
      </div>

      <nav className="shadcn-home__more" aria-label="More documentation">
        <Link href="/tutorials">Tutorials</Link>
        <Link href="/examples">Examples</Link>
        <a href="https://github.com/BoundaryML/baml">GitHub</a>
      </nav>
    </main>
  );
}
