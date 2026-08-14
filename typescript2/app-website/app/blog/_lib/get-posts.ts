'use server';

import fs from 'node:fs/promises';
import path from 'node:path';
import matter from 'gray-matter';

export interface Author {
  name: string;
  imageUrl?: string;
  linkedin?: string;
}

export interface Post {
  title: string;
  description: string;
  slug: string;
  date: string;
  tags: string[];
  body: string;
  isPublished?: boolean;
  lastModified?: number;
  og?: {
    image?: string;
  };
  author?: Author;
  featured?: boolean;
  readingTime?: string;
  firstImage?: string | null;
}

function extractFirstImage(content: string): string | null {
  // Match markdown image syntax: ![alt](url)
  const markdownImageRegex = /!\[.*?\]\(([^)]+)\)/;
  const markdownMatch = content.match(markdownImageRegex);
  if (markdownMatch?.[1]) {
    return markdownMatch[1];
  }

  // Match JSX/HTML image syntax: <img src="url" or <Image src="url"
  // Only match paths that start with / (local images)
  const jsxImageRegex = /<(?:img|Image)[^>]*\ssrc=["']([^"']+)["']/i;
  const jsxMatch = content.match(jsxImageRegex);
  if (jsxMatch?.[1]?.startsWith('/')) {
    return jsxMatch[1];
  }

  return null;
}

function calculateReadingTime(content: string): string {
  // Remove MDX/Markdown syntax and HTML tags for more accurate word count
  const cleanContent = content
    .replace(/```[\s\S]*?```/g, '') // Remove code blocks
    .replace(/`[^`]*`/g, '') // Remove inline code
    .replace(/<[^>]*>/g, '') // Remove HTML tags
    .replace(/!\[.*?\]\(.*?\)/g, '') // Remove images
    .replace(/\[.*?\]\(.*?\)/g, '') // Remove links
    .replace(/#{1,6}\s+/g, '') // Remove headers
    .replace(/\*\*([^*]+)\*\*/g, '$1') // Remove bold
    .replace(/\*([^*]+)\*/g, '$1') // Remove italic
    .replace(/\n+/g, ' ') // Replace newlines with spaces
    .trim();

  const words = cleanContent.split(/\s+/).filter((word) => word.length > 0);
  const wordCount = words.length;

  // Average reading speed is 200-250 words per minute, using 225
  const readingTimeMinutes = Math.ceil(wordCount / 225);

  return `${readingTimeMinutes} min read`;
}

export const getPosts = async () => {
  // Log the full directory structure
  const cwd = process.cwd();
  const postsDirectory = path.join(cwd, 'blog-content');

  try {
    // List all files in the posts directory
    const dirContents = await fs.readdir(postsDirectory, {
      withFileTypes: true,
    });

    const posts = dirContents.map((entry) => entry.name);

    const postsWithMetadata = await Promise.all(
      posts
        .filter(
          (file) =>
            path.extname(file) === '.md' || path.extname(file) === '.mdx',
        )
        .map(async (file) => {
          const filePath = path.join(postsDirectory, file);
          const postContent = await fs.readFile(filePath, 'utf8');
          const matterContent = matter(postContent);
          const data = matterContent.data as Post;

          if (data.isPublished === false) {
            return null;
          }

          // Check if post should be visible based on Pacific Time
          let isPublished = true;
          if (data.date) {
            const publishDate = new Date(data.date);
            // Get current time in Pacific Time
            const pacificTime = new Date(
              new Date().toLocaleString('en-US', {
                timeZone: 'America/Los_Angeles',
              }),
            );

            // Convert publish date to Pacific Time for comparison
            const publishDatePacific = new Date(
              publishDate.toLocaleString('en-US', {
                timeZone: 'America/Los_Angeles',
              }),
            );

            isPublished = publishDatePacific <= pacificTime;
          }

          return {
            ...data,
            body: matterContent.content,
            isPublished,
            lastModified: 0,
            readingTime: calculateReadingTime(matterContent.content),
            firstImage: extractFirstImage(matterContent.content),
          } satisfies Post;
        }),
    );
    const filtered = postsWithMetadata
      .filter((post) => post !== null)
      .sort((a, b) =>
        a && b ? new Date(b.date).getTime() - new Date(a.date).getTime() : 0,
      );

    return filtered;
  } catch (error) {
    console.error('Error reading posts directory:', error);
    console.log(
      'Directory exists?',
      await fs.stat(postsDirectory).catch(() => false),
    );
    throw error;
  }
};

export async function getPost(slug: string) {
  const posts = await getPosts();
  const post = posts.find((post: Post) => post.slug === slug);
  if (!post || !post.isPublished) {
    return null;
  }
  return post;
}

export default getPosts;
