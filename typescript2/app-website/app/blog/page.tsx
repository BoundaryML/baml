import { createMetadata } from '@/app/_lib/metadata';
import { FooterSection } from '@/components/footer-section';
import { Navbar } from '@/components/navbar';
import { BlogContent } from './_components/blog-content';
import { getPosts } from './_lib/get-posts';

export const metadata = createMetadata({
  description: 'Tutorials, updates, and notes from the team building BAML.',
  keywords:
    'BAML blog, AI development, LLM tutorials, machine learning insights, type safety, AI engineering, production AI',
  ogTitle: 'The BAML blog',
  path: '/blog',
  title: 'Blog',
});

export default async function BlogPage() {
  const posts = await getPosts();

  return (
    <div
      style={{
        background: '#FBF7ED',
        color: '#1A1612',
        margin: '0 auto',
        maxWidth: 1600,
        minHeight: '100vh',
        width: '100%',
      }}
    >
      <Navbar />
      <main>
        <BlogContent initialPosts={posts} />
        <FooterSection />
      </main>
    </div>
  );
}
