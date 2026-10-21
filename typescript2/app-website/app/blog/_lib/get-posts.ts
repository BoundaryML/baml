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
  type: 'article' | 'release';
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
  const markdownMatch = content.match(/!\[.*?\]\(([^)]+)\)/);
  if (markdownMatch?.[1]) {
    return markdownMatch[1];
  }

  const jsxMatch = content.match(/<(?:img|Image)[^>]*\ssrc=["']([^"']+)["']/i);
  if (jsxMatch?.[1]?.startsWith('/')) {
    return jsxMatch[1];
  }

  return null;
}

function calculateReadingTime(content: string): string {
  const cleanContent = content
    .replace(/```[\s\S]*?```/g, '')
    .replace(/`[^`]*`/g, '')
    .replace(/<[^>]*>/g, '')
    .replace(/!\[.*?\]\(.*?\)/g, '')
    .replace(/\[.*?\]\(.*?\)/g, '')
    .replace(/#{1,6}\s+/g, '')
    .replace(/\*\*([^*]+)\*\*/g, '$1')
    .replace(/\*([^*]+)\*/g, '$1')
    .replace(/\n+/g, ' ')
    .trim();
  const wordCount = cleanContent
    .split(/\s+/)
    .filter((word) => word.length > 0).length;
  return `${Math.ceil(wordCount / 225)} min read`;
}

async function readPostsFromDirectory(directory: string, type: Post['type']) {
  const entries = await fs.readdir(directory, { withFileTypes: true });

  return Promise.all(
    entries
      .filter(
        (entry) =>
          entry.isFile() && ['.md', '.mdx'].includes(path.extname(entry.name)),
      )
      .map(async (entry) => {
        const postContent = await fs.readFile(
          path.join(directory, entry.name),
          'utf8',
        );
        const matterContent = matter(postContent);
        const data = matterContent.data as Omit<Post, 'body' | 'type'>;

        if (data.isPublished === false) {
          return null;
        }

        let isPublished = true;
        if (data.date) {
          const publishDate = new Date(data.date);
          const pacificTime = new Date(
            new Date().toLocaleString('en-US', {
              timeZone: 'America/Los_Angeles',
            }),
          );
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
          firstImage: extractFirstImage(matterContent.content),
          isPublished,
          lastModified: 0,
          readingTime: calculateReadingTime(matterContent.content),
          type,
        } satisfies Post;
      }),
  );
}

export const getPosts = async () => {
  const cwd = process.cwd();

  try {
    const posts = (
      await Promise.all([
        readPostsFromDirectory(path.join(cwd, 'blog-articles'), 'article'),
        readPostsFromDirectory(path.join(cwd, 'blog-releases'), 'release'),
      ])
    )
      .flat()
      .filter((post) => post !== null)
      .sort((a, b) => new Date(b.date).getTime() - new Date(a.date).getTime());

    return posts;
  } catch (error) {
    console.error('Error reading blog posts:', error);
    throw error;
  }
};

export async function getPost(slug: string) {
  const posts = await getPosts();
  const post = posts.find((candidate) => candidate.slug === slug);
  if (!post?.isPublished) {
    return null;
  }
  return post;
}

export default getPosts;
