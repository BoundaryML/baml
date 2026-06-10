import { readFileSync } from 'node:fs';
import { Sema } from 'async-sema';
import matter from 'gray-matter';
import OpenAI from 'openai';
import z from 'zod';
import { fetchBlogContent } from './external-sitemap';
import { SitemapGenerator } from './sitemap';

export const EMBEDDING_MODEL = 'text-embedding-3-large';

export const openai = new OpenAI({ apiKey: process.env.OPENAI_API_KEY ?? '' });

export const CorpusDocumentSchema = z.object({
  title: z.string(),
  url: z.string(),
  body: z.string(),
  chunkIndex: z.number().optional(),
});

export type CorpusDocument = z.infer<typeof CorpusDocumentSchema>;

export interface EmbeddingWithMetadata {
  embedding: number[];
  document: CorpusDocument;
}

export function estimateTokens(text: string): number {
  return Math.ceil(text.length / 4);
}

export function chunkMarkdown(text: string, maxChunkSize = 3000): string[] {
  const chunks: string[] = [];
  const headerRegex = /^#{1,2}\s+.+$/gm;
  const sections = text.split(headerRegex);
  const headers = text.match(headerRegex) ?? [];

  for (let i = 0; i < sections.length; i++) {
    const header = headers[i - 1] ?? '';
    const content = sections[i].trim();
    if (!content) continue;

    const sectionText = header ? `${header}\n${content}` : content;
    if (sectionText.length <= maxChunkSize) {
      chunks.push(sectionText);
      continue;
    }

    let currentChunk = header ? `${header}\n` : '';
    for (const paragraph of content.split(/\n\s*\n/)) {
      const trimmed = paragraph.trim();
      if (!trimmed) continue;

      if ((currentChunk + trimmed).length > maxChunkSize) {
        if (currentChunk.trim()) chunks.push(currentChunk.trim());
        currentChunk = trimmed;
      } else {
        currentChunk = currentChunk ? `${currentChunk}\n\n${trimmed}` : trimmed;
      }

      if (currentChunk.length > maxChunkSize) {
        let sentenceChunk = '';
        for (const sentence of currentChunk.split(/[.!?]+\s+/)) {
          if ((sentenceChunk + sentence).length > maxChunkSize) {
            if (sentenceChunk.trim()) chunks.push(sentenceChunk.trim());
            sentenceChunk = sentence;
          } else {
            sentenceChunk = sentenceChunk ? `${sentenceChunk}. ${sentence}` : sentence;
          }
        }
        currentChunk = sentenceChunk.trim();
      }
    }
    if (currentChunk.trim()) chunks.push(currentChunk.trim());
  }

  const validated: string[] = [];
  for (const c of chunks) {
    if (estimateTokens(c) <= 7000) {
      validated.push(c);
    } else {
      let wordChunk = '';
      for (const word of c.split(/\s+/)) {
        if ((wordChunk + word).length > 2500) {
          if (wordChunk.trim()) validated.push(wordChunk.trim());
          wordChunk = word;
        } else {
          wordChunk = wordChunk ? `${wordChunk} ${word}` : word;
        }
      }
      if (wordChunk.trim()) validated.push(wordChunk.trim());
    }
  }

  const nonEmpty = validated.filter((c) => c.trim().length > 0);
  return nonEmpty.length > 0 ? nonEmpty : [text.trim()].filter(Boolean);
}

export async function loadCorpusDocs(docsYmlPath: string): Promise<CorpusDocument[]> {
  const sem = new Sema(10);
  const entries = await new SitemapGenerator(docsYmlPath).generateSitemap({ includeBlogPosts: true });
  console.log(`Loaded ${entries.length} sitemap entries`);

  const fern = entries
    .filter((e) => e.type === 'fern')
    .map((e) => ({ title: e.displayTitle, url: e.href, body: matter(readFileSync(e.filepath, 'utf-8')).content }));

  const blog = await Promise.all(
    entries
      .filter((e) => e.type === 'blog')
      .map(async (e) => {
        try {
          await sem.acquire();
          return { title: e.title, url: e.url, body: await fetchBlogContent(e.url) };
        } finally {
          sem.release();
        }
      }),
  );

  const other = entries
    .filter((e) => e.type === 'other')
    .map((e) => ({ title: e.title, url: e.url, body: e.title }));

  console.log('Loaded corpus docs', { fern: fern.length, blog: blog.length, other: other.length });
  return [...fern, ...blog, ...other];
}

export async function embedDocs(docs: CorpusDocument[]): Promise<EmbeddingWithMetadata[]> {
  const chunked = docs.flatMap((doc) =>
    chunkMarkdown(doc.body).map((chunk, chunkIndex) => ({ doc, chunk, chunkIndex })),
  );
  console.log(`Created ${chunked.length} chunks from ${docs.length} documents`);

  return Promise.all(
    chunked.map(async ({ doc, chunk, chunkIndex }) => {
      const tokens = estimateTokens(chunk);
      if (tokens > 7500) {
        console.warn(`⚠️  Chunk ${chunkIndex} for ${doc.url}: ~${tokens} tokens`);
        if (tokens > 8000) chunk = `${chunk.substring(0, 2000)}...`;
      }
      const { data } = await openai.embeddings.create({ model: EMBEDDING_MODEL, input: chunk });
      return { embedding: data[0].embedding, document: { ...doc, body: chunk, chunkIndex } };
    }),
  );
}
