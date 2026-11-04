import type { Metadata } from 'next';
import Link from 'next/link';
import { OG_SITE } from '@/app/_lib/metadata';
import { section } from '@/components/home/analytics';
import { formatPostDate, isoPostDate } from './_lib/format';
import { getPosts } from './_lib/get-posts';
import { BLOG_WRAP, BlogShell } from './_lib/shell';

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
    <BlogShell>
      <section
        {...section('blog_header')}
        className={`${BLOG_WRAP} pt-14 md:pt-20 pb-8 md:pb-10`}
      >
        <header className="flex flex-col lg:flex-row lg:justify-between lg:items-end gap-5 md:gap-8">
          <h1 className="v4-h1">Blog</h1>
          <p className="lg:max-w-[26rem] v4-body text-body">
            How BAML and Boundary work, written by the people building them.
          </p>
        </header>
        <nav
          aria-label="Filter posts"
          className="flex gap-6 mt-10 md:mt-12 text-[15px]"
        >
          {FILTERS.map(([key, label, href]) => (
            <Link
              aria-current={selected === key ? 'page' : undefined}
              className={`tap pb-1.5 border-b transition-colors ${
                selected === key
                  ? 'border-clover text-head'
                  : 'border-transparent text-body hover:text-ink'
              }`}
              href={href}
              key={key}
            >
              {label}
            </Link>
          ))}
        </nav>
      </section>

      {/* Full-width rows: the date, then the post, then its tags and author. */}
      <section
        {...section('blog_posts')}
        className={`${BLOG_WRAP} pb-16 md:pb-24`}
      >
        <ul className="border-t border-ink">
          {posts.map((p) => (
            <li className="border-b border-rule" key={p.slug}>
              <Link
                className="group grid lg:grid-cols-[170px_minmax(0,1fr)_200px] gap-y-3 lg:gap-x-10 py-7 md:py-8"
                href={`/blog/${p.slug}`}
              >
                <span className="v4-caption text-muted lg:pt-1.5">
                  <time dateTime={isoPostDate(p.date)}>
                    {formatPostDate(p.date)}
                  </time>
                </span>

                <span className="flex flex-col gap-2.5">
                  <h2 className="max-w-[34ch] font-display font-medium text-[22px] md:text-[26px] leading-[1.2] tracking-[-0.03em] text-head group-hover:text-clover transition-colors">
                    {p.title}
                  </h2>
                  <span className="max-w-[62ch] text-[16px] leading-[1.5] text-body">
                    {p.description}
                  </span>
                </span>

                <span className="v4-caption flex flex-col gap-1.5 text-muted lg:pt-1.5 lg:text-right">
                  <span className="capitalize">{p.tags.join(', ')}</span>
                  {p.author && <span>{p.author.name}</span>}
                </span>
              </Link>
            </li>
          ))}
        </ul>
      </section>
    </BlogShell>
  );
}
