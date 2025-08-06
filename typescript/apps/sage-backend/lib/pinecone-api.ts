import { existsSync, readFileSync, writeFileSync } from 'node:fs';
import { Pinecone } from '@pinecone-database/pinecone';
import { Sema } from 'async-sema';
import matter from 'gray-matter';
import { chunk } from 'lodash';
import OpenAI from 'openai';
import z from 'zod';
import { fetchBlogContent } from './external-sitemap';
import { type SitemapEntry, SitemapGenerator } from './sitemap';

const EMBEDDING_MODEL = 'text-embedding-3-large';
const PINECONE_INDEX_NAME =
  process.env.NODE_ENV === 'production' ? 'ask-baml-prod' : 'ask-baml-dev';
console.log('Using pinecone index:', PINECONE_INDEX_NAME);

const openaiClient = new OpenAI({
  apiKey: process.env.OPENAI_API_KEY ?? '',
});

const pineconeClient = new Pinecone({
  apiKey: process.env.PINECONE_API_KEY ?? '',
});

const pineconeIndex = pineconeClient.Index(PINECONE_INDEX_NAME);

const CorpusDocumentSchema = z.object({
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

/**
 * Rough token estimation (1 token ≈ 4 characters for English text)
 */
export function estimateTokens(text: string): number {
  return Math.ceil(text.length / 4);
}

/**
 * Chunk markdown content into smaller pieces for embedding
 */
export function chunkMarkdown(text: string, maxChunkSize = 3000): string[] {
  const chunks: string[] = [];

  // First, split by major headers (H1, H2)
  const headerRegex = /^#{1,2}\s+.+$/gm;
  const sections = text.split(headerRegex);
  const headers = text.match(headerRegex) || [];

  for (let i = 0; i < sections.length; i++) {
    const header = headers[i - 1] || '';
    const content = sections[i].trim();

    if (!content) continue;

    const sectionText = header ? `${header}\n${content}` : content;

    // If section is small enough, add it directly
    if (sectionText.length <= maxChunkSize) {
      chunks.push(sectionText);
      continue;
    }

    // If section is too large, split by paragraphs
    const paragraphs = content.split(/\n\s*\n/);
    let currentChunk = header ? `${header}\n` : '';

    for (const paragraph of paragraphs) {
      const trimmedParagraph = paragraph.trim();
      if (!trimmedParagraph) continue;

      // If adding this paragraph would exceed limit, start new chunk
      if ((currentChunk + trimmedParagraph).length > maxChunkSize) {
        if (currentChunk.trim()) {
          chunks.push(currentChunk.trim());
        }
        currentChunk = trimmedParagraph;
      } else {
        currentChunk = currentChunk ? `${currentChunk}\n\n${trimmedParagraph}` : trimmedParagraph;
      }

      // If even a single paragraph is too large, split by sentences
      if (currentChunk.length > maxChunkSize) {
        const sentences = currentChunk.split(/[.!?]+\s+/);
        let sentenceChunk = '';

        for (const sentence of sentences) {
          if ((sentenceChunk + sentence).length > maxChunkSize) {
            if (sentenceChunk.trim()) {
              chunks.push(sentenceChunk.trim());
            }
            sentenceChunk = sentence;
          } else {
            sentenceChunk = sentenceChunk ? `${sentenceChunk}. ${sentence}` : sentence;
          }
        }

        if (sentenceChunk.trim()) {
          currentChunk = sentenceChunk;
        } else {
          currentChunk = '';
        }
      }
    }

    if (currentChunk.trim()) {
      chunks.push(currentChunk.trim());
    }
  }

  // Final validation: ensure no chunk exceeds token limits
  const validatedChunks: string[] = [];
  for (const chunk of chunks) {
    if (estimateTokens(chunk) > 7000) {
      // Leave some buffer below 8192
      // Force split by character count as last resort
      const words = chunk.split(/\s+/);
      let wordChunk = '';

      for (const word of words) {
        if ((wordChunk + word).length > 2500) {
          // Very conservative
          if (wordChunk.trim()) {
            validatedChunks.push(wordChunk.trim());
          }
          wordChunk = word;
        } else {
          wordChunk = wordChunk ? `${wordChunk} ${word}` : word;
        }
      }

      if (wordChunk.trim()) {
        validatedChunks.push(wordChunk.trim());
      }
    } else {
      validatedChunks.push(chunk);
    }
  }

  return validatedChunks.filter((chunk) => chunk.length > 50); // Remove very small chunks
}

/**
 * Generate embeddings for text chunks and prepare for Pinecone upsert
 */
async function generateEmbeddingsForDocs(docs: CorpusDocument[]): Promise<EmbeddingWithMetadata[]> {
  const chunkedDocs: {
    doc: CorpusDocument;
    chunk: string;
    chunkIndex: number;
  }[] = docs.flatMap((doc: CorpusDocument) => {
    const chunks = chunkMarkdown(doc.body);
    return chunks.map((chunk, chunkIndex) => ({
      doc,
      chunk,
      chunkIndex,
    }));
  });

  console.log(`Created ${chunkedDocs.length} chunks from ${docs.length} documents`);

  // Generate embeddings for all chunks
  const embeddingsWithMetadata: EmbeddingWithMetadata[] = await Promise.all(
    chunkedDocs.map(async ({ doc, chunk, chunkIndex }) => {
      // Validate chunk size before sending to OpenAI
      const estimatedTokens = estimateTokens(chunk);
      if (estimatedTokens > 7500) {
        console.warn(
          `⚠️  Chunk ${chunkIndex} for ${doc.url} is large: ~${estimatedTokens} tokens (${chunk.length} chars)`,
        );
        // Truncate if still too large
        if (estimatedTokens > 8000) {
          chunk = `${chunk.substring(0, 2000)}...`;
          console.warn('✂️  Truncated chunk to avoid API error');
        }
      }

      const embeddingResponse = await openaiClient.embeddings.create({
        model: 'text-embedding-3-large',
        input: chunk,
      });

      return {
        embedding: embeddingResponse.data[0].embedding,
        document: {
          url: doc.url,
          body: chunk,
          title: doc.title,
          chunkIndex,
        },
      };
    }),
  );

  return embeddingsWithMetadata;
}

/**
 * Upsert embeddings to Pinecone in batches
 */
async function upsertToPinecone(embeddingsWithMetadata: EmbeddingWithMetadata[]): Promise<void> {
  // Prepare records for Pinecone using the combined data
  const records = embeddingsWithMetadata.map(({ embedding, document }) => ({
    id: `${document.url}::chunk-${document.chunkIndex}`,
    values: embedding,
    metadata: document,
  }));

  // Use lodash chunk to create batches of 100 records
  const batches = chunk(records, 100);
  console.log(`Upserting ${records.length} records in ${batches.length} batches`);

  // Execute all batches in parallel
  await Promise.all(
    batches.map(async (batch, index) => {
      await pineconeIndex.upsert(batch);
      console.log(`Upserted batch ${index + 1}/${batches.length} with ${batch.length} records`);
    }),
  );

  console.log(`Successfully upserted all ${records.length} records to Pinecone`);
}

/**
 * Main function to populate Pinecone with documents from sitemap
 */
export async function populatePinecone(docsYmlPath: string): Promise<void> {
  const BLOG_FETCH_CONCURRENCY = new Sema(10);
  const USE_CACHE = { readFromFile: false, writeToFile: false };

  const sitemapEntries = await (async () => {
    const SITEMAP_CACHE_PATH = './sitemap.json';
    if (USE_CACHE.readFromFile && existsSync(SITEMAP_CACHE_PATH)) {
      return JSON.parse(readFileSync(SITEMAP_CACHE_PATH, 'utf-8')) as SitemapEntry[];
    }
    const generator = new SitemapGenerator(docsYmlPath);
    const sitemap = await generator.generateSitemap({
      includeBlogPosts: true,
    });
    if (USE_CACHE.writeToFile) {
      writeFileSync(SITEMAP_CACHE_PATH, JSON.stringify(sitemap, null, 2));
    }
    return sitemap;
  })();
  console.log(`Loaded ${sitemapEntries.length} sitemap entries`);

  const fernCorpusDocs = sitemapEntries
    .filter((entry) => entry.type === 'fern')
    .map((entry) => ({
      title: entry.displayTitle,
      url: entry.href,
      body: matter(readFileSync(entry.filepath, 'utf-8')).content,
    }));
  const blogCorpusDocs = await Promise.all(
    sitemapEntries
      .filter((entry) => entry.type === 'blog')
      .map(async (entry) => {
        try {
          await BLOG_FETCH_CONCURRENCY.acquire();
          return {
            title: entry.title,
            url: entry.url,
            body: await fetchBlogContent(entry.url),
          };
        } finally {
          BLOG_FETCH_CONCURRENCY.release();
        }
      }),
  );
  const otherCorpusDocs = sitemapEntries
    .filter((entry) => entry.type === 'other')
    .map((entry) => ({
      title: entry.title,
      url: entry.url,
      body: entry.title,
    }));
  console.log('Loaded corpus documents', {
    fern: fernCorpusDocs.length,
    blog: blogCorpusDocs.length,
    other: otherCorpusDocs.length,
  });

  const embeddingsWithMetadata = await generateEmbeddingsForDocs([
    ...fernCorpusDocs,
    ...blogCorpusDocs,
    ...otherCorpusDocs,
  ]);
  console.log(`Computed embeddings for ${embeddingsWithMetadata.length} chunks`);

  const beforeStats = await pineconeIndex.describeIndexStats();
  console.log('Before stats', beforeStats);
  const deleted = await pineconeIndex.deleteAll();
  console.log('Deleted old embeddings from Pinecone', deleted);
  await upsertToPinecone(embeddingsWithMetadata);
  console.log('Upserted new embeddings to Pinecone');
  const afterStats = await pineconeIndex.describeIndexStats();
  console.log('After stats', afterStats);
  console.log(`✅ Successfully populated Pinecone with ${embeddingsWithMetadata.length} chunks`);
}

/**
 * Search Pinecone for relevant documents using vector similarity
 */
export async function searchPinecone(query: string) {
  const results = await pineconeIndex.query({
    vector: await openaiClient.embeddings
      .create({
        model: EMBEDDING_MODEL,
        input: query,
      })
      .then((res) => res.data[0].embedding),
    topK: 7,
    includeMetadata: true,
  });
  console.info(`Found ${results.matches.length} matches in pinecone for query`);
  return results.matches.map((match) => CorpusDocumentSchema.parse(match.metadata));
}
