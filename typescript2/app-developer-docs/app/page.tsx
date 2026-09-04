import {
  ArrowRight,
  BookOpen,
  Braces,
  Cloud,
  Code2,
  History,
  TerminalSquare,
} from 'lucide-react';
import Link from 'next/link';

import { PageHeader } from '@/components/page-header';
import { documentationMetadata } from '@/lib/metadata';

const sections = [
  {
    description:
      'Learn the language, follow the book, explore syntax, and browse standard packages.',
    href: '/baml',
    icon: Braces,
    title: 'BAML',
  },
  {
    description:
      'Install the toolchain and understand commands, configuration, and local workflows.',
    href: '/cli',
    icon: TerminalSquare,
    title: 'BAML CLI',
  },
  {
    description:
      'The developing cloud platform for operating BAML applications and workflows.',
    href: '/bcs',
    icon: Cloud,
    title: 'Boundary Cloud Services',
  },
  {
    description:
      'Goal-oriented guides that connect BAML with real application architectures.',
    href: '/tutorials',
    icon: BookOpen,
    title: 'Tutorials',
  },
  {
    description:
      'Focused examples you can inspect, adapt, and use as starting points.',
    href: '/examples',
    icon: Code2,
    title: 'Examples',
  },
  {
    description:
      'Follow language, toolchain, and package changes across releases.',
    href: '/changelog',
    icon: History,
    title: 'Changelog',
  },
];

export const dynamic = 'force-static';
export const metadata = documentationMetadata({
  description:
    'Technical documentation for BAML, the BAML CLI, and Boundary Cloud Services.',
  path: '/',
  title: 'BAML Developer Documentation',
});

export default function HomePage() {
  return (
    <div className="flex flex-1 flex-col">
      <PageHeader
        actions={
          <>
            <Link className="button-primary" href="/baml/get-started">
              Get Started <ArrowRight aria-hidden="true" className="size-4" />
            </Link>
            <Link className="button-secondary" href="/baml">
              Explore BAML
            </Link>
          </>
        }
        description="One technical home for the BAML language, its CLI, practical workflows, and Boundary Cloud Services."
        eyebrow={
          <Link
            className="inline-flex items-center gap-2 rounded-full border bg-muted px-3 py-1 text-xs font-medium text-muted-foreground hover:text-foreground"
            href="/changelog"
          >
            BAML developer documentation
            <ArrowRight aria-hidden="true" className="size-3.5" />
          </Link>
        }
        title="Build reliable AI applications with BAML."
      />
      <section className="container-wrapper flex-1 p-0">
        <div className="container overflow-hidden px-0 lg:max-w-none">
          <div className="relative flex w-full max-w-none flex-col overflow-hidden bg-muted p-6 dark:bg-background">
            <div className="relative z-10 mx-auto grid w-full gap-6 md:max-w-3xl md:grid-cols-2 lg:max-w-none lg:grid-cols-3 xl:max-w-[1600px]">
              {sections.map((section) => {
                const Icon = section.icon;
                return (
                  <Link
                    className="group min-h-64 rounded-2xl border bg-card p-6 text-card-foreground shadow-sm transition-colors hover:bg-accent"
                    href={section.href}
                    key={section.href}
                  >
                    <div className="mb-16 flex items-center justify-between">
                      <Icon
                        aria-hidden="true"
                        className="size-5 text-[var(--docs-purple)]"
                      />
                      <ArrowRight
                        aria-hidden="true"
                        className="size-4 text-muted-foreground transition-transform group-hover:translate-x-1 group-hover:text-foreground"
                      />
                    </div>
                    <h2 className="text-sm font-semibold tracking-tight">
                      {section.title}
                    </h2>
                    <p className="mt-2 max-w-sm text-sm leading-6 text-muted-foreground">
                      {section.description}
                    </p>
                  </Link>
                );
              })}
            </div>
          </div>
        </div>
      </section>
    </div>
  );
}
