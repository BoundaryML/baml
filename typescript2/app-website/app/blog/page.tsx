import type { Metadata } from 'next';
import { FooterSection } from '@/components/footer-section';
import { Navbar } from '@/components/navbar';
import { BlogContent } from './_components/blog-content';
import { getPosts } from './_lib/get-posts';

export async function generateMetadata(): Promise<Metadata> {
  const baseUrl = process.env.SITE_URL || 'https://boundaryml.com';
  const blogUrl = `${baseUrl}/blog`;

  return {
    title: 'BAML Blog - Insights, Tutorials, and AI Development Updates',
    description: 'Explore the latest insights, tutorials, and updates from the BAML team. Learn best practices for building type-safe, production-ready AI applications.',
    alternates: {
      canonical: blogUrl,
    },
    keywords: 'BAML blog, AI development, LLM tutorials, machine learning insights, type safety, AI engineering, production AI',
    openGraph: {
      title: 'BAML Blog - Insights, Tutorials, and AI Development Updates',
      description: 'Explore the latest insights, tutorials, and updates from the BAML team. Learn best practices for building type-safe, production-ready AI applications.',
      url: blogUrl,
      siteName: 'BAML',
      type: 'website',
      images: [
        {
          url: `${baseUrl}/baml-og-background.png`,
          width: 1200,
          height: 630,
          alt: 'BAML Blog - AI Development Insights',
        },
      ],
    },
    twitter: {
      card: 'summary_large_image',
      title: 'BAML Blog - Insights, Tutorials, and AI Development Updates',
      description: 'Explore the latest insights, tutorials, and updates from the BAML team.',
      images: [`${baseUrl}/baml-og-background.png`],
      creator: '@boundaryml',
      site: '@boundaryml',
    },
  };
}

export default async function BlogPage() {
  const posts = await getPosts();

  return (
    <div
      style={{
        background: '#ffffff',
        color: '#1A1612',
        width: '100%',
        maxWidth: 1600,
        margin: '0 auto',
        minHeight: '100vh',
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
