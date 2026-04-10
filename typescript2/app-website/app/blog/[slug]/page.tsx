import { formatDistanceToNow } from 'date-fns';
import { ArrowLeft, Calendar, Clock, Tag } from 'lucide-react';
import Image from 'next/image';
import Link from 'next/link';
import { notFound } from 'next/navigation';
import { FooterSection } from '@/components/footer-section';
import { Navbar } from '@/components/navbar';
import { Button } from '@/components/ui/button';
import { getPost, getPosts } from '../_lib/get-posts';
import { PostBody } from './content';

interface BlogPostPageProps {
  params: Promise<{
    slug: string;
  }>;
}

// Generate static params for all blog posts
export async function generateStaticParams() {
  const posts = await getPosts();
  return posts.map((post) => ({
    slug: post.slug,
  }));
}

// Generate metadata for SEO
export async function generateMetadata({ params }: BlogPostPageProps) {
  const { slug } = await params;
  const post = await getPost(slug);

  if (!post) {
    return {
      title: 'Post Not Found | BAML Blog',
      description: 'The requested blog post could not be found.',
    };
  }

  const baseUrl = process.env.SITE_URL || 'https://boundaryml.com';
  const postUrl = `${baseUrl}/blog/${post.slug}`;

  // Enhanced title with site branding
  const fullTitle = `${post.title} | BAML Blog`;

  // Use the dynamic OG image API route
  const ogImage = `${baseUrl}/api/og?slug=${encodeURIComponent(post.slug)}`;

  // Build comprehensive keywords
  const keywords = [
    ...post.tags,
    'BAML',
    'AI development',
    'LLM',
    'machine learning',
    'type safety',
    'AI engineering'
  ].join(', ');

  return {
    title: fullTitle,
    description: post.description,
    alternates: {
      canonical: postUrl,
    },
    authors: post.author ? [{ name: post.author.name }] : undefined,
    keywords: keywords,
    openGraph: {
      title: post.title,
      description: post.description,
      url: postUrl,
      siteName: 'BAML',
      type: 'article',
      publishedTime: post.date,
      authors: post.author ? [post.author.name] : undefined,
      tags: post.tags,
      images: [
        {
          url: ogImage,
          width: 1200,
          height: 630,
          alt: `${post.title} - BAML Blog`,
        },
      ],
    },
    twitter: {
      card: 'summary_large_image',
      title: post.title,
      description: post.description,
      images: [ogImage],
      creator: '@boundaryml',
      site: '@boundaryml',
    },
  };
}

const getCategoryStyles = (category: string) => {
  // Convert tag to camelCase format for style lookup
  const normalizeCategory = (cat: string) => {
    if (cat.toLowerCase() === 'launch week') return 'LaunchWeek';
    return cat.charAt(0).toUpperCase() + cat.slice(1).toLowerCase();
  };

  const normalizedCategory = normalizeCategory(category);

  const styles = {
    All: { backgroundColor: '#f1f5f9', color: '#1e293b' },
    Announcements: { backgroundColor: '#dbeafe', color: '#1e40af' },
    Engineering: { backgroundColor: '#fed7aa', color: '#ea580c' },
    LaunchWeek: { backgroundColor: '#fce7f3', color: '#be185d' },
    Research: { backgroundColor: '#f3e8ff', color: '#7c3aed' },
    Tutorials: { backgroundColor: '#dcfce7', color: '#166534' },
  };
  return styles[normalizedCategory as keyof typeof styles] || styles['All'];
};

const formatCategoryForDisplay = (category: string) => {
  if (category.toLowerCase() === 'launch week' || category === 'LaunchWeek')
    return 'Launch Week';
  return category.charAt(0).toUpperCase() + category.slice(1).toLowerCase();
};

export default async function BlogPostPage({ params }: BlogPostPageProps) {
  const { slug } = await params;
  const post = await getPost(slug);

  if (!post) {
    notFound();
  }

  return (
    <div className="max-w-7xl mx-auto border-x relative">
      <Navbar />
      <main className="flex flex-col items-center justify-center min-h-screen w-full">
        {/* Back Button */}
        <section className="w-full px-4 py-8">
          <div className="mx-auto max-w-4xl">
            <Link href="/blog">
              <Button className="group lg:mb-8" variant="ghost">
                <ArrowLeft className="mr-2 h-4 w-4 transition-transform group-hover:-translate-x-1" />
                Back to Blog
              </Button>
            </Link>
          </div>
        </section>

        {/* Article Header */}
        <section className="w-full px-4 lg:py-8">
          <div className="mx-auto max-w-4xl">
            <div className="flex items-center gap-4 text-sm text-muted-foreground mb-6">
              <span
                className="flex items-center gap-1 px-3 py-1 rounded-full text-sm font-medium"
                style={getCategoryStyles(post.tags[0])}
              >
                <Tag className="h-4 w-4" />
                {formatCategoryForDisplay(post.tags[0])}
              </span>
              <span className="flex items-center gap-1">
                <Calendar className="h-4 w-4" />
                {formatDistanceToNow(new Date(post.date), { addSuffix: true })}
              </span>
              <span className="flex items-center gap-1">
                <Clock className="h-4 w-4" />
                {post.readingTime}
              </span>
            </div>

            <h1 className="text-4xl md:text-5xl font-bold tracking-tight mb-4">
              {post.title}
            </h1>

            <p className="text-xl text-muted-foreground mb-8">
              {post.description}
            </p>

            {post.author && (
              <div className="flex items-center gap-4 pb-8 border-b">
                {post.author.imageUrl && (
                  <Image
                    alt={post.author.name}
                    className="w-12 h-12 rounded-full object-cover"
                    height={512}
                    src={post.author.imageUrl}
                    width={512}
                  />
                )}
                <div>
                  <p className="font-semibold">{post.author.name}</p>
                  {post.author.linkedin && (
                    <Link
                      className="text-sm text-muted-foreground hover:text-primary"
                      href={post.author.linkedin}
                      rel="noopener noreferrer"
                      target="_blank"
                    >
                      LinkedIn
                    </Link>
                  )}
                </div>
              </div>
            )}
          </div>
        </section>

        {/* Article Content */}
        <section className="w-full px-4 py-8">
          <PostBody>{post.body}</PostBody>
        </section>

        {/* Back to Blog CTA */}
        <section className="w-full px-4 py-8">
          <div className="mx-auto max-w-4xl text-center">
            <Link href="/blog">
              <Button size="lg">
                <ArrowLeft className="mr-2 h-4 w-4" />
                Back to All Posts
              </Button>
            </Link>
          </div>
        </section>

        <FooterSection />
      </main>
    </div>
  );
}
