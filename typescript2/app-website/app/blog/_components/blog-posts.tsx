import { type Post } from '../_lib/get-posts';
import { BlogPostsGrid } from './blog-posts-grid';
import { FeaturedPost } from './featured-post';

interface BlogPostsProps {
  posts: Post[];
}

export function BlogPosts({ posts }: BlogPostsProps) {
  const featuredPosts = posts.filter((post) => post.featured);
  const regularPosts = posts.filter((post) => !post.featured);

  return (
    <>
      {/* Featured Post */}
      {featuredPosts.length > 0 && (
        <section className="w-full px-4">
          <div className="mx-auto max-w-6xl">
            {featuredPosts.map((post) => (
              <FeaturedPost key={post.slug} post={post} />
            ))}
          </div>
        </section>
      )}

      {/* Blog Posts Grid */}
      <section className="w-full px-4 py-20">
        <div className="mx-auto max-w-6xl">
          <BlogPostsGrid posts={regularPosts} />
        </div>
      </section>
    </>
  );
}
