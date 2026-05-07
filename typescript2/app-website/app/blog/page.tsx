import type { Metadata } from 'next';
import { FooterSection } from '@/components/footer-section';
import { Navbar } from '@/components/navbar';
import { BlogContent } from './_components/blog-content';
import { getPosts } from './_lib/get-posts';

export async function generateMetadata(): Promise<Metadata> {
  const baseUrl = process.env.SITE_URL || 'https://boundaryml.com';
  const blogUrl = `${baseUrl}/blog`;

  return {
    alternates: {
      canonical: blogUrl,
    },
    description:
      'Explore the latest insights, tutorials, and updates from the BAML team. Learn best practices for building type-safe, production-ready AI applications.',
    keywords:
      'BAML blog, AI development, LLM tutorials, machine learning insights, type safety, AI engineering, production AI',
    openGraph: {
      description:
        'Explore the latest insights, tutorials, and updates from the BAML team. Learn best practices for building type-safe, production-ready AI applications.',
      images: [
        {
          alt: 'BAML Blog - AI Development Insights',
          height: 630,
          url: `${baseUrl}/baml-og-background.png`,
          width: 1200,
        },
      ],
      siteName: 'BAML',
      title: 'BAML Blog - Insights, Tutorials, and AI Development Updates',
      type: 'website',
      url: blogUrl,
    },
    title: 'BAML Blog - Insights, Tutorials, and AI Development Updates',
    twitter: {
      card: 'summary_large_image',
      creator: '@boundaryml',
      description:
        'Explore the latest insights, tutorials, and updates from the BAML team.',
      images: [`${baseUrl}/baml-og-background.png`],
      site: '@boundaryml',
      title: 'BAML Blog - Insights, Tutorials, and AI Development Updates',
    },
  };
}

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
