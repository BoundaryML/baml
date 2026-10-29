import type { Metadata } from 'next';
import Link from 'next/link';
import { cta } from '@/components/boundary/analytics';
import Footer from '@/components/boundary/footer';
import Nav from '@/components/boundary/nav';
import { CONTAINER, CtaBand, OG_SITE } from '@/components/boundary/page-parts';
import PageTransition from '@/components/boundary/page-transition';
import { formatPostDate, isoPostDate } from './_lib/format';
import { getPosts } from './_lib/get-posts';

const DESCRIPTION =
  'Notes from the team building BAML and Boundary: tutorials, research and release notes.';

export const metadata: Metadata = {
  alternates: { canonical: '/blog' },
  description: DESCRIPTION,
  openGraph: {
    ...OG_SITE,
    description: DESCRIPTION,
    title: 'Blog · Boundary',
    type: 'website',
  },
  title: 'Blog',
};

// `?tags=article` and `?tags=release` filter the list; /changelog redirects to the releases.
const FILTERS = [
  ['all', 'Everything', '/blog'],
  ['article', 'Articles', '/blog?tags=article'],
  ['release', 'Releases', '/blog?tags=release'],
] as const;

export default async function BlogIndex({
  searchParams,
}: {
  searchParams: Promise<{ tags?: string | string[] }>;
}) {
  const { tags } = await searchParams;
  const selected = tags === 'article' || tags === 'release' ? tags : 'all';
  const posts = (await getPosts()).filter(
    (p) => p.isPublished && (selected === 'all' || p.type === selected),
  );

  return (
    <>
      <Nav />
      <PageTransition id={'blog'}>
        <main className="flex-1">
          {/* Same header shape as the case studies index: title left, one line of context right. */}
          <section className={`${CONTAINER} pt-14 md:pt-22 pb-8 md:pb-10`}>
            <header className="flex flex-col lg:flex-row lg:justify-between lg:items-end gap-5 md:gap-8">
              <h1 className="max-w-205 h2-display">Blog</h1>
              <p className="lg:w-100 shrink-0 text-[17px] leading-[27px] text-ink-2">
                How BAML and Boundary work, written by the people building them.
              </p>
            </header>
            <nav
              aria-label="Filter posts"
              className="flex gap-6 mt-10 md:mt-12 text-sm"
            >
              {FILTERS.map(([key, label, href]) => (
                <Link
                  aria-current={selected === key ? 'page' : undefined}
                  className={`focus-ring pb-1.5 border-b transition-colors ${
                    selected === key
                      ? 'border-ink text-ink'
                      : 'border-transparent text-ink-2 hover:text-ink'
                  }`}
                  href={href}
                  key={key}
                  {...cta(`filter_${key}`, 'blog')}
                >
                  {label}
                </Link>
              ))}
            </nav>
          </section>

          {/* Full-width rows rather than a narrow list beside a dead column: date, then the post,
              then its tags and author. Hairlines and the heavy top rule come from Faq and Loop. */}
          <section className={`${CONTAINER} pb-12 md:pb-18`}>
            <ul className="border-t border-ink">
              {posts.map((p) => (
                <li className="border-b border-hair" key={p.slug}>
                  <Link
                    className="focus-ring group grid lg:grid-cols-[170px_minmax(0,1fr)_220px] gap-y-3 lg:gap-x-10 py-7 md:py-8"
                    href={`/blog/${p.slug}`}
                  >
                    <span className="text-sm leading-5 text-ink-2 lg:pt-1">
                      <time dateTime={isoPostDate(p.date)}>
                        {formatPostDate(p.date)}
                      </time>
                    </span>

                    <span className="flex flex-col gap-2.5">
                      <h2 className="max-w-[34ch] font-display font-medium text-[21px] md:text-[23px] leading-[1.26] tracking-[-0.03em] text-ink group-hover:text-accent transition-colors">
                        {p.title}
                      </h2>
                      <span className="max-w-[62ch] text-[15px] leading-6 text-ink-2">
                        {p.description}
                      </span>
                    </span>

                    <span className="flex flex-col gap-1 text-sm leading-5 text-ink-2 lg:pt-1 lg:text-right">
                      <span className="capitalize">{p.tags.join(', ')}</span>
                      {p.author && <span>{p.author.name}</span>}
                    </span>
                  </Link>
                </li>
              ))}
            </ul>
          </section>

          <CtaBand />
        </main>
      </PageTransition>
      <Footer />
    </>
  );
}
