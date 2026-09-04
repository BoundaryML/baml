import { format } from 'date-fns';
import { ArrowRight } from 'lucide-react';
import Link from 'next/link';
import type { Post } from '../_lib/get-posts';

const filters = [
  { href: '/blog', label: 'Everything', type: 'all' },
  { href: '/blog?tags=article', label: 'Articles', type: 'article' },
  { href: '/blog?tags=release', label: 'Releases', type: 'release' },
] as const;

interface HeroSectionProps {
  latestRelease?: Post;
  selectedType: 'all' | 'article' | 'release';
}

export function HeroSection({ latestRelease, selectedType }: HeroSectionProps) {
  return (
    <section className="border-b border-[#D9D3C4] px-6 py-16 md:px-12 md:py-24">
      <div className="mx-auto max-w-5xl">
        <h1 className="text-5xl font-semibold tracking-tight md:text-7xl">
          Blog
        </h1>
        <p className="mt-5 max-w-2xl text-lg leading-8 text-[#5C5852]">
          Articles and release notes from the team building BAML.
        </p>

        {latestRelease && (
          <Link
            className="group mt-12 block border border-[#D9D3C4] bg-white p-6 transition-colors hover:border-[#1A1612] md:p-8"
            href={`/blog/${latestRelease.slug}`}
          >
            <div className="flex flex-wrap items-center justify-between gap-3 text-sm text-[#6B665F]">
              <span className="font-medium uppercase tracking-[0.12em]">
                Latest release
              </span>
              <time dateTime={latestRelease.date}>
                {format(new Date(latestRelease.date), 'MMMM d, yyyy')}
              </time>
            </div>
            <div className="mt-8 grid gap-4 md:grid-cols-[1fr_auto] md:items-end">
              <div>
                <h2 className="text-3xl font-semibold tracking-tight md:text-4xl">
                  {latestRelease.title}
                </h2>
                <p className="mt-3 max-w-2xl leading-7 text-[#5C5852]">
                  {latestRelease.description}
                </p>
              </div>
              <span className="flex items-center gap-2 text-sm font-medium">
                Read release notes
                <ArrowRight
                  className="transition-transform group-hover:translate-x-1"
                  size={16}
                />
              </span>
            </div>
          </Link>
        )}

        <nav
          aria-label="Filter blog posts"
          className="mt-12 flex flex-wrap gap-2"
        >
          {filters.map((filter) => {
            const active = selectedType === filter.type;
            return (
              <Link
                aria-current={active ? 'page' : undefined}
                className={`border px-4 py-2 text-sm font-medium transition-colors ${active ? 'border-[#1A1612] bg-[#1A1612] text-white' : 'border-[#D9D3C4] bg-transparent hover:border-[#1A1612]'}`}
                href={filter.href}
                key={filter.type}
              >
                {filter.label}
              </Link>
            );
          })}
        </nav>
      </div>
    </section>
  );
}
