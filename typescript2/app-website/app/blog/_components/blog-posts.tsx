import { type Post } from '../_lib/get-posts';
import { BlogPostsGrid } from './blog-posts-grid';
import { FeaturedPost } from './featured-post';

interface BlogPostsProps {
  posts: Post[];
}

const BORDER = '#D9D3C4';
const EYEBROW = '#8A8580';
const INK = '#1A1612';

export function BlogPosts({ posts }: BlogPostsProps) {
  const featuredPosts = posts.filter((post) => post.featured);
  const regularPosts = posts.filter((post) => !post.featured);

  return (
    <>
      {featuredPosts.length > 0 && (
        <section
          style={{
            width: '100%',
            padding: '64px 48px 0',
          }}
        >
          <div style={{ maxWidth: 1200, margin: '0 auto' }}>
            {featuredPosts.map((post) => (
              <FeaturedPost key={post.slug} post={post} />
            ))}
          </div>
        </section>
      )}

      <section
        style={{
          width: '100%',
          padding: '64px 48px 96px',
        }}
      >
        <div style={{ maxWidth: 1200, margin: '0 auto' }}>
          <div
            style={{
              display: 'flex',
              alignItems: 'baseline',
              gap: 16,
              marginBottom: 32,
              paddingBottom: 16,
              borderBottom: `1px solid ${BORDER}`,
            }}
          >
            <p
              style={{
                fontSize: 12,
                fontWeight: 500,
                letterSpacing: '0.14em',
                textTransform: 'uppercase',
                color: EYEBROW,
                margin: 0,
              }}
            >
              All posts
            </p>
            <h2
              style={{
                fontSize: 'clamp(1.5rem, 2.5vw, 2rem)',
                fontWeight: 600,
                lineHeight: 1.15,
                letterSpacing: '-0.02em',
                margin: 0,
                color: INK,
              }}
            >
              The archive.
            </h2>
          </div>
          <BlogPostsGrid posts={regularPosts} />
        </div>
      </section>
    </>
  );
}
