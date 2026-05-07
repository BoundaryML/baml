import type { Post } from '../_lib/get-posts';
import { BlogPostsGrid } from './blog-posts-grid';
import { FeaturedPost } from './featured-post';

interface BlogPostsProps {
  posts: Post[];
}

const BORDER = '#D9D3C4';
const EYEBROW = '#8A8580';

export function BlogPosts({ posts }: BlogPostsProps) {
  const featuredPosts = posts.filter((post) => post.featured);
  const regularPosts = posts.filter((post) => !post.featured);

  return (
    <>
      {featuredPosts.length > 0 && (
        <section
          style={{
            padding: '64px 48px 0',
            width: '100%',
          }}
        >
          <div style={{ margin: '0 auto', maxWidth: 1200 }}>
            {featuredPosts.map((post) => (
              <FeaturedPost key={post.slug} post={post} />
            ))}
          </div>
        </section>
      )}

      <section
        style={{
          padding: '64px 48px 96px',
          width: '100%',
        }}
      >
        <div style={{ margin: '0 auto', maxWidth: 1200 }}>
          <div
            style={{
              alignItems: 'baseline',
              borderBottom: `1px solid ${BORDER}`,
              display: 'flex',
              gap: 16,
              marginBottom: 32,
              paddingBottom: 16,
            }}
          >
            <p
              style={{
                color: EYEBROW,
                fontSize: 12,
                fontWeight: 500,
                letterSpacing: '0.14em',
                margin: 0,
                textTransform: 'uppercase',
              }}
            >
              All posts
            </p>
          </div>
          <BlogPostsGrid posts={regularPosts} />
        </div>
      </section>
    </>
  );
}
