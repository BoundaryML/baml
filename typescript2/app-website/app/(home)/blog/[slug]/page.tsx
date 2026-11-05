import type { Metadata } from 'next';
import Link from 'next/link';
import { notFound } from 'next/navigation';
import { ArticleStructuredData } from '@/components/structured-data';
import { formatPostDate, isoPostDate } from '../_lib/format';
import { getPost, getPosts } from '../_lib/get-posts';
import { BLOG_WRAP, BlogShell } from '../_lib/shell';
import { isListed, topicLabel, topicsOf } from '../_lib/topics';
import { PostBody } from './content';

interface BlogPostPageProps {
  params: Promise<{ slug: string }>;
}

export async function generateStaticParams() {
  const posts = await getPosts();
  return posts.map((post) => ({ slug: post.slug }));
}

export async function generateMetadata({
  params,
}: BlogPostPageProps): Promise<Metadata> {
  const { slug } = await params;
  const post = await getPost(slug);
  if (!post) {
    return {
      description: 'The requested blog post could not be found.',
      title: 'Post not found',
    };
  }

  // The per-post share card (app/api/og), resolved against the layout's metadataBase.
  const ogImage = `/api/og?slug=${encodeURIComponent(post.slug)}`;

  return {
    alternates: { canonical: `/blog/${post.slug}` },
    authors: post.author ? [{ name: post.author.name }] : undefined,
    description: post.description,
    keywords: [...post.tags, 'BAML', 'Boundary', 'AI engineering'].join(', '),
    openGraph: {
      authors: post.author ? [post.author.name] : undefined,
      description: post.description,
      images: [{ alt: post.title, height: 630, url: ogImage, width: 1200 }],
      publishedTime: isoPostDate(post.date),
      siteName: 'Boundary',
      tags: post.tags,
      title: post.title,
      type: 'article',
      url: `/blog/${post.slug}`,
    },
    title: post.title,
    twitter: {
      card: 'summary_large_image',
      description: post.description,
      images: [ogImage],
      title: post.title,
    },
  };
}

export default async function BlogPost({ params }: BlogPostPageProps) {
  const { slug } = await params;
  const post = await getPost(slug);
  if (!post) notFound();

  // "Read next": the two newest listed posts that share this post's first topic, topped up with the
  // newest listed posts of any topic. A release suggests other releases.
  const others = (await getPosts()).filter(
    (p) => isListed(p) && p.slug !== post.slug,
  );
  const topic = topicsOf(post)[0];
  const related = others.filter((p) =>
    post.type === 'release'
      ? p.type === 'release'
      : p.type === 'article' &&
        topic !== undefined &&
        topicsOf(p).includes(topic),
  );
  const readNext = [
    ...related,
    ...others.filter((p) => !related.includes(p)),
  ].slice(0, 2);

  return (
    <>
      <ArticleStructuredData
        authorName={post.author?.name}
        datePublished={post.date}
        description={post.description}
        headline={post.title}
        image={`/api/og?slug=${encodeURIComponent(post.slug)}`}
        url={`/blog/${post.slug}`}
      />
      <BlogShell>
        <div className={BLOG_WRAP}>
          {/* One measure for the whole article, about 66 characters of 20px body text, so the rules
                under the header line up with the end of the body copy. */}
          <div className="max-w-[720px] mx-auto">
            <nav aria-label="Breadcrumb" className="pt-8 md:pt-12">
              <Link
                className="tap group inline-flex items-center gap-2 text-[15px] text-body hover:text-ink transition-colors"
                href="/blog"
              >
                <span
                  aria-hidden
                  className="transition-transform duration-200 ease-out group-hover:-translate-x-0.5"
                >
                  ←
                </span>
                Blog
              </Link>
            </nav>

            <article className="pt-9 md:pt-12">
              <header className="flex flex-col gap-5 pb-9 md:pb-11 border-b border-rule">
                <span className="v4-caption flex flex-wrap items-center gap-x-2 gap-y-1 text-muted">
                  <time dateTime={isoPostDate(post.date)}>
                    {formatPostDate(post.date)}
                  </time>
                  {post.author && (
                    <>
                      <span aria-hidden>·</span>
                      <span>{post.author.name}</span>
                    </>
                  )}
                  {post.readingTime && (
                    <>
                      <span aria-hidden>·</span>
                      <span>{post.readingTime}</span>
                    </>
                  )}
                </span>
                <h1 className="v4-h2">{post.title}</h1>
                <p className="v4-body text-body">{post.description}</p>
                {post.archived && (
                  <p className="border-l-2 border-rule pl-4 text-[15px] leading-[1.5] text-body">
                    This is an older post, kept for reference. BAML has changed
                    since it was written, so check{' '}
                    <Link
                      className="underline underline-offset-4 hover:text-ink"
                      href="/quickstart"
                    >
                      the quickstart
                    </Link>{' '}
                    for how it works today.
                  </p>
                )}
              </header>

              <div className="pt-8 pb-12 md:pb-16">
                <PostBody>{post.body}</PostBody>
              </div>

              <footer className="flex flex-col gap-5 pt-7 pb-14 md:pb-20 border-t border-ink">
                <h2 className="v4-caption text-muted">Read next</h2>
                <ul className="grid sm:grid-cols-2 gap-6">
                  {readNext.map((p) => (
                    <li key={p.slug}>
                      <Link
                        className="group flex flex-col gap-1.5"
                        href={`/blog/${p.slug}`}
                      >
                        <span className="font-display font-medium text-[19px] leading-[1.3] tracking-[-0.02em] text-head group-hover:text-clover transition-colors">
                          {p.title}
                        </span>
                        <span className="v4-caption flex flex-wrap gap-x-4 text-muted">
                          {topicLabel(p) && <span>{topicLabel(p)}</span>}
                          {p.readingTime && <span>{p.readingTime}</span>}
                        </span>
                      </Link>
                    </li>
                  ))}
                </ul>
              </footer>
            </article>
          </div>
        </div>
      </BlogShell>
    </>
  );
}
