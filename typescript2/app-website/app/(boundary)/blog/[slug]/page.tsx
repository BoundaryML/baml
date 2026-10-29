import type { Metadata } from 'next';
import Link from 'next/link';
import { notFound } from 'next/navigation';
import Footer from '@/components/boundary/footer';
import Nav from '@/components/boundary/nav';
import { BackLink, CONTAINER, CtaBand } from '@/components/boundary/page-parts';
import PageTransition from '@/components/boundary/page-transition';
import { ArticleStructuredData } from '@/components/structured-data';
import { formatPostDate, isoPostDate } from '../_lib/format';
import { getPost, getPosts } from '../_lib/get-posts';
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

  // Neighbours in the newest-first list: "Previous" is the older post, "Next" the newer one.
  const posts = (await getPosts()).filter((p) => p.isPublished);
  const i = posts.findIndex((p) => p.slug === slug);
  const prev = posts[i + 1];
  const next = posts[i - 1];

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
      <Nav />
      <PageTransition id={`blog/${slug}`}>
        <main className="flex-1">
          <div className={CONTAINER}>
            {/* One measure for the whole article: ~68ch at 17px, so the rules under the header line
                up with the end of the body copy. */}
            <div className="max-w-[68ch] mx-auto">
              <BackLink href="/blog" label="Blog" />

              <article className="pt-9 md:pt-12">
                <header className="flex flex-col gap-5 pb-9 md:pb-11 border-b border-hair">
                  <span className="flex flex-wrap items-center gap-x-2 gap-y-1 text-sm leading-5 text-ink-2">
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
                  <h1 className="h2-display">{post.title}</h1>
                  <p className="text-[19px] leading-[30px] text-ink-2">
                    {post.description}
                  </p>
                </header>

                <div className="pt-8 pb-12 md:pb-16">
                  <PostBody>{post.body}</PostBody>
                </div>

                <footer className="flex flex-col gap-3 pt-7 pb-14 md:pb-20 border-t border-ink">
                  <span className="text-sm leading-5 text-ink-2">
                    More from the blog
                  </span>
                  {/* A lone neighbour takes the row rather than sitting beside an empty half. */}
                  <div className="flex flex-col sm:flex-row sm:justify-between gap-6">
                    {[
                      { label: 'Previous', post: prev },
                      { label: 'Next', post: next },
                    ]
                      .flatMap(({ label, post: p }) =>
                        p ? [{ label, p }] : [],
                      )
                      .map(({ label, p }) => (
                        <Link
                          className={`focus-ring group flex flex-col gap-1.5 sm:max-w-[48%] ${
                            label === 'Next'
                              ? 'sm:ml-auto sm:items-end sm:text-right'
                              : ''
                          }`}
                          href={`/blog/${p.slug}`}
                          key={label}
                        >
                          <span className="text-sm leading-5 text-ink-2">
                            {label}
                          </span>
                          <span className="font-display font-medium text-[17px] leading-6 tracking-[-0.03em] text-ink group-hover:text-accent transition-colors">
                            {p.title}
                          </span>
                        </Link>
                      ))}
                  </div>
                </footer>
              </article>
            </div>
          </div>

          <CtaBand />
        </main>
      </PageTransition>
      <Footer />
    </>
  );
}
