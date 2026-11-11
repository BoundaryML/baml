import type { Metadata } from 'next';
import Link from 'next/link';
import { OG_SITE } from '@/app/_lib/metadata';
import { section } from '@/components/home/analytics';
import { PostArt } from './_lib/art';
import { formatPostDate, isoPostDate } from './_lib/format';
import { getPosts, type Post } from './_lib/get-posts';
import { BLOG_WRAP, BlogShell } from './_lib/shell';
import {
  isOnIndex,
  isTopic,
  LAUNCH_WEEK_PREFIX,
  TOPICS,
  type Topic,
  topicLabel,
  topicsOf,
} from './_lib/topics';

const DESCRIPTION =
  'Notes from the team building BAML and Boundary: tutorials, research and release notes.';

export const metadata: Metadata = {
  alternates: {
    canonical: '/blog',
    types: { 'application/rss+xml': '/blog/rss.xml' },
  },
  description: DESCRIPTION,
  openGraph: {
    ...OG_SITE,
    description: DESCRIPTION,
    title: 'Blog · Boundary',
    type: 'website',
  },
  title: 'Blog',
};

// The blog index. It opens on the newest article, then a hand-picked "Start here" row (posts with
// `featured: true`), the latest release notes, and every other article by date. A filter
// (`?tags=research`, `?tags=release`, ...) changes only that last list; the sections above it stay. `?tags=article` is the unfiltered
// view, kept for old links; /changelog redirects to `?tags=release`. Archived posts aren't listed.

type Filter = 'all' | 'release' | Topic;

const FILTERS: { href: string; key: Filter; label: string }[] = [
  { href: '/blog', key: 'all', label: 'Everything' },
  ...TOPICS.map((t) => ({
    href: `/blog?tags=${t.key}`,
    key: t.key,
    label: t.label,
  })),
  { href: '/blog?tags=release', key: 'release', label: 'Releases' },
];

const H2 =
  'font-display font-medium text-[22px] md:text-[24px] leading-[1.2] tracking-[-0.02em] text-head';
const ROW_GRID =
  'grid lg:grid-cols-[170px_minmax(0,1fr)_200px] gap-y-3 lg:gap-x-10 py-7 md:py-8';
const ROW_TITLE =
  'max-w-[34ch] font-display font-medium text-[22px] md:text-[26px] leading-[1.2] tracking-[-0.03em] text-head transition-colors';

/** The filter named by `?tags=`, or the unfiltered view. */
function readFilter(tags: string | string[] | undefined): Filter {
  const key = Array.isArray(tags) ? tags[0] : tags;
  if (key === 'release') return 'release';
  return key && isTopic(key) ? key : 'all';
}

/** One row of the list: a post, or Launch Week's posts gathered into one entry. */
type Row = { kind: 'post'; post: Post } | { kind: 'group'; days: Post[] };

/** The list's rows, with Launch Week's posts folded into one row where its announcement sat. */
function toRows(posts: Post[]): Row[] {
  const days = posts
    .filter((p) => p.slug.startsWith(LAUNCH_WEEK_PREFIX))
    .sort((a, b) => a.slug.localeCompare(b.slug));
  const rows: Row[] = [];
  for (const post of posts) {
    if (!post.slug.startsWith(LAUNCH_WEEK_PREFIX)) {
      rows.push({ kind: 'post', post });
    } else if (post === days[0]) {
      rows.push({ days, kind: 'group' });
    }
  }
  return rows;
}

/** A post's date as a `<time>`. */
function PostDate({ post }: { post: Post }) {
  return (
    <time dateTime={isoPostDate(post.date)}>{formatPostDate(post.date)}</time>
  );
}

/** A post's row in the list: the date, then the title and description, then its topic, author, and
 *  reading time. */
function PostRow({ post }: { post: Post }) {
  const topic = topicLabel(post);
  return (
    <li className="border-b border-rule">
      <Link className={`group ${ROW_GRID}`} href={`/blog/${post.slug}`}>
        <span className="v4-caption text-muted lg:pt-1.5">
          <PostDate post={post} />
        </span>
        <span className="flex flex-col gap-2.5">
          <h3 className={`${ROW_TITLE} group-hover:text-clover`}>
            {post.title}
          </h3>
          <span className="max-w-[62ch] text-[16px] leading-[1.5] text-body">
            {post.description}
          </span>
        </span>
        <span className="v4-caption flex flex-col gap-1.5 text-muted lg:pt-1.5 lg:text-right">
          {topic && <span>{topic}</span>}
          {post.author && <span>{post.author.name}</span>}
          {post.readingTime && <span>{post.readingTime}</span>}
        </span>
      </Link>
    </li>
  );
}

/** Launch Week as one row: the title links to the announcement, and each day links to its post. */
function LaunchWeekRow({ days }: { days: Post[] }) {
  const [announcement, ...rest] = days;
  return (
    <li className="border-b border-rule">
      <div className={ROW_GRID}>
        <span className="v4-caption text-muted lg:pt-1.5">
          <PostDate post={announcement} />
        </span>
        <span className="flex flex-col gap-3">
          <h3>
            <Link
              className={`${ROW_TITLE} hover:text-clover`}
              href={`/blog/${announcement.slug}`}
            >
              BAML Launch Week
            </Link>
          </h3>
          <span className="max-w-[62ch] text-[16px] leading-[1.5] text-body">
            Five days of launches in January 2025.
          </span>
          <ul className="flex flex-col gap-1.5 text-[15px]">
            {rest.map((day, i) => (
              <li key={day.slug}>
                <Link
                  className="text-ink-2 hover:text-clover transition-colors"
                  href={`/blog/${day.slug}`}
                >
                  <span className="text-muted">Day {i + 1}:</span>{' '}
                  {day.description}
                </Link>
              </li>
            ))}
          </ul>
        </span>
        <span className="v4-caption text-muted lg:pt-1.5 lg:text-right">
          Announcements
        </span>
      </div>
    </li>
  );
}

/** The newest article, large, at the top of the unfiltered index. */
function Lead({ post }: { post: Post }) {
  const topic = topicLabel(post);
  return (
    <section
      {...section('blog_lead')}
      aria-labelledby="blog-latest"
      className={`${BLOG_WRAP} pb-14 md:pb-20`}
    >
      <Link
        className="group grid lg:grid-cols-[minmax(0,1fr)_minmax(0,0.85fr)] gap-8 lg:gap-14 border-t border-ink pt-8 md:pt-10"
        href={`/blog/${post.slug}`}
      >
        <span className="flex flex-col gap-5">
          <span className="v4-caption flex flex-wrap gap-x-4 gap-y-1 text-muted">
            <PostDate post={post} />
            {topic && <span>{topic}</span>}
          </span>
          <h2
            className="v4-h2 max-w-[18em] text-head group-hover:text-clover transition-colors"
            id="blog-latest"
          >
            {post.title}
          </h2>
          <span className="max-w-[36rem] v4-body text-body">
            {post.description}
          </span>
          <span className="v4-caption flex flex-wrap gap-x-4 gap-y-1 text-muted">
            {post.author && <span>{post.author.name}</span>}
            {post.readingTime && <span>{post.readingTime}</span>}
          </span>
        </span>
        <PostArt
          className="rounded-2xl border border-rule"
          id="lead"
          post={post}
        />
      </Link>
    </section>
  );
}

/** The hand-picked posts to read first: the ones marked `featured`. */
function StartHere({ posts }: { posts: Post[] }) {
  return (
    <section
      {...section('blog_start_here')}
      aria-labelledby="blog-start-here"
      className={`${BLOG_WRAP} pb-14 md:pb-20`}
    >
      <h2 className={H2} id="blog-start-here">
        Start here
      </h2>
      <ul className="mt-6 grid sm:grid-cols-2 lg:grid-cols-4 gap-3">
        {posts.map((p) => (
          <li key={p.slug}>
            <Link
              className="group flex h-full flex-col gap-3 rounded-xl border border-rule bg-raised p-5 transition-colors hover:border-clover/50"
              href={`/blog/${p.slug}`}
            >
              <span className="-mx-5 -mt-5 mb-1 block overflow-hidden rounded-t-xl border-b border-rule">
                <PostArt id={`start-${p.slug}`} post={p} />
              </span>
              <span className="v4-caption text-muted">{topicLabel(p)}</span>
              <span className="font-display font-medium text-[19px] leading-[1.25] tracking-[-0.02em] text-head group-hover:text-clover transition-colors">
                {p.title}
              </span>
              <span className="text-[15px] leading-[1.5] text-body line-clamp-3">
                {p.description}
              </span>
              {p.readingTime && (
                <span className="v4-caption mt-auto pt-1 text-muted">
                  {p.readingTime}
                </span>
              )}
            </Link>
          </li>
        ))}
      </ul>
    </section>
  );
}

/** The latest few releases, with a link to all of them. */
function ReleaseNotes({ posts }: { posts: Post[] }) {
  return (
    <section
      {...section('blog_releases')}
      aria-labelledby="blog-releases"
      className={`${BLOG_WRAP} pb-14 md:pb-20`}
    >
      <div className="flex flex-wrap items-baseline justify-between gap-4">
        <h2 className={H2} id="blog-releases">
          Release notes
        </h2>
        <Link
          className="tap text-[15px] text-body hover:text-ink transition-colors"
          href="/blog?tags=release"
        >
          All release notes
        </Link>
      </div>
      <ul className="mt-6 grid sm:grid-cols-3 gap-3">
        {posts.map((p) => (
          <li key={p.slug}>
            <Link
              className="group flex h-full flex-col gap-1.5 rounded-xl border border-rule px-5 py-4 transition-colors hover:border-clover/50"
              href={`/blog/${p.slug}`}
            >
              <span className="font-display font-medium text-[18px] text-head group-hover:text-clover transition-colors">
                {p.title}
              </span>
              <span className="v4-caption text-muted">
                <PostDate post={p} />
              </span>
            </Link>
          </li>
        ))}
      </ul>
    </section>
  );
}

/** The blog index: the newest article, the start-here picks, release notes, then the filtered list. */
export default async function BlogIndex({
  searchParams,
}: {
  searchParams: Promise<{ tags?: string | string[] }>;
}) {
  const { tags } = await searchParams;
  const selected = readFilter(tags);
  const posts = (await getPosts()).filter(isOnIndex);
  const articles = posts.filter((p) => p.type === 'article');
  const releases = posts.filter((p) => p.type === 'release');

  // The lead, Start here, and release notes stay put under every filter, so picking one changes only
  // the list below them. "All posts" means all: the lead is listed there too.
  const lead = articles.at(0);
  const featured = articles.filter((p) => p.featured && p !== lead);
  const listed =
    selected === 'all'
      ? articles
      : selected === 'release'
        ? releases
        : articles.filter((p) => topicsOf(p).includes(selected));

  return (
    <BlogShell>
      <section
        {...section('blog_header')}
        className={`${BLOG_WRAP} pt-14 md:pt-20 pb-10 md:pb-14`}
      >
        <header className="flex flex-col lg:flex-row lg:justify-between lg:items-end gap-5 md:gap-8">
          <h1 className="v4-h1">Blog</h1>
          <div className="lg:max-w-[26rem] flex flex-col gap-3">
            <a
              className="tap self-start text-[15px] text-body underline decoration-rule underline-offset-4 hover:text-ink transition-colors"
              href="/blog/rss.xml"
            >
              Subscribe with RSS
            </a>
          </div>
        </header>
      </section>

      {lead && <Lead post={lead} />}
      {featured.length > 0 && <StartHere posts={featured} />}
      {releases.length > 0 && <ReleaseNotes posts={releases.slice(0, 3)} />}

      <section
        {...section('blog_posts')}
        aria-labelledby="blog-all"
        className={`${BLOG_WRAP} pb-16 md:pb-24`}
      >
        <h2 className={lead ? H2 : 'sr-only'} id="blog-all">
          All posts
        </h2>
        <nav
          aria-label="Filter posts"
          className={`flex flex-wrap gap-x-6 gap-y-2 text-[15px] ${lead ? 'mt-6' : ''}`}
        >
          {FILTERS.filter(
            // A topic with nothing on the index has no tab, unless it's the one asked for.
            ({ key }) =>
              key === 'all' ||
              key === selected ||
              (key === 'release'
                ? releases.length > 0
                : articles.some((p) => topicsOf(p).includes(key))),
          ).map(({ href, key, label }) => (
            <Link
              aria-current={selected === key ? 'page' : undefined}
              className={`tap pb-1.5 border-b transition-colors ${
                selected === key
                  ? 'border-clover text-head'
                  : 'border-transparent text-body hover:text-ink'
              }`}
              href={href}
              key={key}
              scroll={false}
            >
              {label}
            </Link>
          ))}
        </nav>
        <ul className="mt-6 border-t border-ink">
          {toRows(listed).map((row) =>
            row.kind === 'post' ? (
              <PostRow key={row.post.slug} post={row.post} />
            ) : (
              <LaunchWeekRow days={row.days} key="launch-week" />
            ),
          )}
        </ul>
      </section>
    </BlogShell>
  );
}
