import { format } from 'date-fns';
import { ArrowRight } from 'lucide-react';
import Link from 'next/link';
import type { Post } from '../_lib/get-posts';

interface BlogListProps {
  posts: Post[];
  selectedType: 'all' | 'article' | 'release';
}

export function BlogList({ posts, selectedType }: BlogListProps) {
  const postsByYear = Map.groupBy(posts, (post) =>
    new Date(post.date).getFullYear(),
  );
  const label =
    selectedType === 'all'
      ? 'All posts'
      : selectedType === 'article'
        ? 'Articles'
        : 'Releases';

  return (
    <section className="px-6 py-16 md:px-12 md:py-24">
      <div className="mx-auto max-w-5xl">
        <div className="flex items-baseline justify-between gap-4 border-b border-[#D9D3C4] pb-4">
          <h2 className="text-sm font-medium uppercase tracking-[0.12em]">
            {label}
          </h2>
          <span className="text-sm text-[#6B665F]">
            {posts.length} {posts.length === 1 ? 'post' : 'posts'}
          </span>
        </div>

        {posts.length === 0 ? (
          <p className="py-12 text-[#5C5852]">No posts here yet.</p>
        ) : (
          <div>
            {[...postsByYear.entries()].map(([year, yearPosts]) => (
              <section
                className="grid border-b border-[#D9D3C4] py-8 md:grid-cols-[120px_1fr]"
                key={year}
              >
                <h3 className="mb-4 text-xl font-semibold md:mb-0">{year}</h3>
                <div>
                  {yearPosts.map((post) => (
                    <Link
                      className="group grid gap-2 border-t border-[#E5E0D5] py-5 first:border-t-0 md:grid-cols-[110px_1fr_auto] md:items-center"
                      href={`/blog/${post.slug}`}
                      key={post.slug}
                    >
                      <time
                        className="text-sm text-[#6B665F]"
                        dateTime={post.date}
                      >
                        {format(new Date(post.date), 'MMM d')}
                      </time>
                      <div>
                        <span className="text-xs font-medium uppercase tracking-[0.1em] text-[#6B665F]">
                          {post.type}
                        </span>
                        <h4 className="mt-1 text-lg font-medium transition-colors group-hover:text-purple-700">
                          {post.title}
                        </h4>
                      </div>
                      <ArrowRight
                        className="hidden transition-transform group-hover:translate-x-1 md:block"
                        size={17}
                      />
                    </Link>
                  ))}
                </div>
              </section>
            ))}
          </div>
        )}
      </div>
    </section>
  );
}
