'use server';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { Pinecone } from '@pinecone-database/pinecone';
import * as cheerio from 'cheerio';
import OpenAI from 'openai';

const openaiClient = new OpenAI({
  apiKey: process.env.OPENAI_API_KEY ?? '',
});

const pineconeClient = new Pinecone({
  apiKey: process.env.PINECONE_API_KEY ?? '',
});

const pineconeIndex = pineconeClient.Index('baml-index-sage');

interface SitemapEntry {
  title: string;
  path?: string;
  url?: string;
  type: 'internal' | 'external';
  slug?: string;
  description?: string;
  section?: string;
  [key: string]: any;
}

interface FernDoc {
  slug: string;
  path: string;
  body: string;
  title: string;
  chunkIndex?: number;
}

interface EmbeddingWithMetadata {
  embedding: number[];
  document: FernDoc;
}

// Rough token estimation (1 token ≈ 4 characters for English text)
function estimateTokens(text: string): number {
  return Math.ceil(text.length / 4);
}

function chunkMarkdown(text: string, maxChunkSize = 3000): string[] {
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
        currentChunk = currentChunk
          ? `${currentChunk}\n\n${trimmedParagraph}`
          : trimmedParagraph;
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
            sentenceChunk = sentenceChunk
              ? `${sentenceChunk}. ${sentence}`
              : sentence;
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

// Helper function to extract clean text content from HTML
function extractTextFromHtml(html: string): string {
  const $ = cheerio.load(html);

  // Remove unwanted elements
  $(
    'script, style, nav, header, footer, .navigation, .sidebar, .ads, .cookie-banner, .header, .footer',
  ).remove();

  // Try to find main content area
  let content = '';
  const contentSelectors = [
    'main article',
    'main',
    'article',
    '.post-content',
    '.entry-content',
    '.blog-content',
    '.content',
    '[role="main"]',
    '.post-body',
    '.article-content',
  ];

  for (const selector of contentSelectors) {
    const element = $(selector);
    if (element.length) {
      const text = element.text().trim();
      if (text.length > content.length) {
        content = text;
      }
    }
  }

  // If no main content found, try body with unwanted elements removed
  if (!content) {
    $(
      'header, footer, nav, aside, .header, .footer, .nav, .sidebar, .menu, .navigation',
    ).remove();
    content = $('body').text().trim();
  }

  // Clean up whitespace and normalize
  content = content
    .replace(/\s+/g, ' ')
    .replace(/\n\s*\n/g, '\n')
    .trim();

  return content;
}

// Helper function to fetch and clean blog content
async function fetchBlogContent(url: string): Promise<string> {
  try {
    console.log(`Fetching blog content from: ${url}`);
    const response = await fetch(url);
    if (!response.ok) {
      throw new Error(`HTTP error! status: ${response.status}`);
    }

    const html = await response.text();
    const content = extractTextFromHtml(html);

    if (!content || content.length < 100) {
      throw new Error('Could not extract meaningful content from blog post');
    }

    console.log(
      `✓ Successfully extracted ${content.length} characters from ${url}`,
    );
    return content;
  } catch (error) {
    console.error(`✗ Error fetching blog content from ${url}:`, error);
    // Return a minimal fallback content
    return `Blog post: ${url}\nTitle: ${url.split('/').pop()?.replace(/-/g, ' ') || 'Blog Post'}`;
  }
}

// Helper function to read internal doc content
function readInternalDocContent(docPath: string): string {
  try {
    // Assume docs are in the fern directory relative to the sage directory
    const fullPath = join('../fern', docPath);
    const content = readFileSync(fullPath, 'utf8');

    // Remove frontmatter if present
    const frontmatterRegex = /^---\s*\n[\s\S]*?\n---\s*\n/;
    const cleanContent = content.replace(frontmatterRegex, '').trim();

    return cleanContent;
  } catch (error) {
    console.error(`Error reading internal doc ${docPath}:`, error);
    return `Document: ${docPath}`;
  }
}

export async function populatePinecone() {
  // Read sitemap.json which contains all documentation sources
  const sitemap: SitemapEntry[] = JSON.parse(
    readFileSync('./sitemap.json', 'utf8'),
  );

  console.log(`Found ${sitemap.length} total entries in sitemap`);

  // Separate internal docs and external blog posts
  const internalDocs = sitemap.filter((entry) => entry.type === 'internal');
  const externalBlogs = sitemap.filter((entry) => entry.type === 'external');

  console.log(
    `Processing ${internalDocs.length} internal docs and ${externalBlogs.length} external blog posts`,
  );

  // Delete all existing records once before starting
  try {
    await pineconeIndex.deleteAll();
    console.log('Cleared existing Pinecone records');
  } catch (e) {
    console.log('No existing records to delete');
  }

  // Process internal docs first
  const internalFernDocs: FernDoc[] = [];

  for (const entry of internalDocs) {
    if (!entry.path || !entry.slug) {
      console.warn(
        `Skipping internal doc without path or slug: ${entry.title}`,
      );
      continue;
    }

    try {
      const content = readInternalDocContent(entry.path);
      internalFernDocs.push({
        slug: entry.slug,
        path: entry.path,
        body: content,
        title: entry.title,
      });
      console.log(`✓ Processed internal doc: ${entry.title}`);
    } catch (error) {
      console.error(`✗ Failed to process internal doc ${entry.title}:`, error);
    }
  }

  // Process external blog posts
  const externalFernDocs: FernDoc[] = [];

  for (const entry of externalBlogs) {
    if (!entry.url) {
      console.warn(`Skipping external entry without URL: ${entry.title}`);
      continue;
    }

    try {
      const content = await fetchBlogContent(entry.url);
      // Use the full URL as the slug for external content
      const slug = entry.url;

      externalFernDocs.push({
        slug: slug,
        path: entry.url,
        body: content,
        title: entry.title,
      });
      console.log(`✓ Processed external blog: ${entry.title}`);
    } catch (error) {
      console.error(`✗ Failed to process external blog ${entry.title}:`, error);
    }
  }

  // Combine all documents
  const allDocs = [...internalFernDocs, ...externalFernDocs];
  console.log(`Total documents to process: ${allDocs.length}`);

  // Process docs in batches of 10
  for (let i = 0; i < allDocs.length; i += 10) {
    const batch = allDocs.slice(i, i + 10);
    console.log(
      `Processing batch ${Math.floor(i / 10) + 1}/${Math.ceil(allDocs.length / 10)}`,
    );

    // First, create all chunks for each document
    const chunkedDocs: { doc: FernDoc; chunk: string; chunkIndex: number }[] =
      batch.flatMap((doc: FernDoc) => {
        const chunks = chunkMarkdown(doc.body);
        return chunks.map((chunk, chunkIndex) => ({
          doc,
          chunk,
          chunkIndex,
        }));
      });

    console.log(
      `Created ${chunkedDocs.length} chunks from ${batch.length} documents`,
    );

    // Then, generate embeddings for all chunks
    const embeddingsWithMetadata: EmbeddingWithMetadata[] = await Promise.all(
      chunkedDocs.map(async ({ doc, chunk, chunkIndex }) => {
        // Validate chunk size before sending to OpenAI
        const estimatedTokens = estimateTokens(chunk);
        if (estimatedTokens > 7500) {
          console.warn(
            `⚠️  Chunk ${chunkIndex} for ${doc.slug} is large: ~${estimatedTokens} tokens (${chunk.length} chars)`,
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
            slug: doc.slug,
            path: doc.path,
            body: chunk,
            title: doc.title,
            chunkIndex,
          },
        };
      }),
    );

    // Prepare records for Pinecone using the combined data
    const records = embeddingsWithMetadata.map(({ embedding, document }) => ({
      id: `${document.slug.replace(/[^a-zA-Z0-9-_]/g, '_')}-chunk-${document.chunkIndex}`,
      values: embedding,
      metadata: {
        slug: document.slug,
        path: document.path,
        body: document.body,
        title: document.title,
      },
    }));

    // Upsert in batches of 100
    for (let j = 0; j < records.length; j += 100) {
      const upsertBatch = records.slice(j, j + 100);
      await pineconeIndex.upsert(upsertBatch);
      console.log(`Upserted ${upsertBatch.length} records to Pinecone`);
    }
  }

  console.log(
    `✅ Successfully populated Pinecone with ${allDocs.length} documents`,
  );
}

// Test function to verify populate works with a small subset
export async function testPopulatePinecone() {
  try {
    // Read sitemap and take a small sample
    const sitemap: SitemapEntry[] = JSON.parse(
      readFileSync('./sitemap.json', 'utf8'),
    );

    const sampleInternal = sitemap
      .filter((entry) => entry.type === 'internal')
      .slice(0, 2);
    const sampleExternal = sitemap
      .filter((entry) => entry.type === 'external')
      .slice(0, 1);

    console.log(
      `Testing with ${sampleInternal.length} internal docs and ${sampleExternal.length} external blogs`,
    );

    // Test internal doc processing
    for (const entry of sampleInternal) {
      if (!entry.path) {
        console.error(`✗ Internal doc "${entry.title}" has no path`);
        continue;
      }
      try {
        const content = readInternalDocContent(entry.path);
        console.log(
          `✓ Internal doc "${entry.title}": ${content.length} characters`,
        );
      } catch (error) {
        console.error(`✗ Failed to read internal doc "${entry.title}"`);
      }
    }

    // Test external blog processing
    for (const entry of sampleExternal) {
      if (!entry.url) {
        console.error(`✗ External blog "${entry.title}" has no URL`);
        continue;
      }
      try {
        const content = await fetchBlogContent(entry.url);
        console.log(
          `✓ External blog "${entry.title}": ${content.length} characters`,
        );
        console.log(content.slice(0, 200));
      } catch (error) {
        console.error(`✗ Failed to fetch external blog "${entry.title}"`);
      }
    }

    console.log('✅ Test completed successfully');
    return true;
  } catch (error) {
    console.error('❌ Test failed:', error);
    return false;
  }
}

// RAG using pinecone
export async function searchPinecone(query: string, count = 5) {
  const results = await pineconeIndex.query({
    vector: await openaiClient.embeddings
      .create({
        model: 'text-embedding-3-large',
        input: query,
      })
      .then((res) => res.data[0].embedding),
    topK: count,
    includeMetadata: true,
  });
  console.log('Got matches', results.matches.length);
  // console.log(results.matches);
  return results.matches;
}

// Copy vectors from baml-index to baml-index-sage
async function copyPineconeIndex() {
  const sourceIndex = pineconeClient.Index('baml-index');
  const targetIndex = pineconeClient.Index('baml-index-sage');

  try {
    // Get index statistics to understand the data size
    const stats = await sourceIndex.describeIndexStats();
    console.log('Source index stats:', stats);

    const totalVectors = stats.totalRecordCount || 0;
    if (totalVectors === 0) {
      console.log('No vectors found in source index');
      return;
    }

    console.log(
      `Copying ${totalVectors} vectors from baml-index to baml-index-sage...`,
    );

    // Clear the target index first
    // try {
    //   await targetIndex.deleteAll();
    //   console.log('Cleared target index');
    // } catch (e) {
    //   console.log('Target index was already empty or error clearing:', e);
    // }

    // We need to query in batches since Pinecone doesn't have a "list all" operation
    // We'll use a dummy query to get all vectors
    const batchSize = 1000;
    let copiedCount = 0;

    // Get all unique namespaces first
    const namespaces = Object.keys(stats.namespaces || { '': stats });

    for (const namespace of namespaces) {
      console.log(`Processing namespace: ${namespace || 'default'}`);

      // Query with a zero vector to get vectors (this is a workaround)
      // Since we can't list all vectors, we'll query with high topK
      const queryOptions = {
        topK: Math.min(batchSize, 10000), // Pinecone max is 10000
        includeMetadata: true,
        includeValues: true,
      };

      // We need to provide a vector for the query, so we'll use a dummy vector
      // with the same dimensions. Let's get the dimension from the first vector
      let vectorDimension = 3072; // Default for text-embedding-3-large

      try {
        // Try to get a sample vector to determine dimensions
        const sampleQueryOptions = {
          ...queryOptions,
          vector: new Array(vectorDimension).fill(0),
          topK: 1,
        };

        const sampleQuery = namespace
          ? await sourceIndex.namespace(namespace).query(sampleQueryOptions)
          : await sourceIndex.query(sampleQueryOptions);

        if (sampleQuery.matches.length > 0) {
          vectorDimension =
            sampleQuery.matches[0].values?.length || vectorDimension;
        }
      } catch (e) {
        console.log(
          'Could not determine vector dimension, using default:',
          vectorDimension,
        );
      }

      // Query all vectors in this namespace
      const finalQueryOptions = {
        ...queryOptions,
        vector: new Array(vectorDimension).fill(0),
        topK: 10000, // Get as many as possible
      };

      const queryResult = namespace
        ? await sourceIndex.namespace(namespace).query(finalQueryOptions)
        : await sourceIndex.query(finalQueryOptions);

      const vectors = queryResult.matches;
      console.log(
        `Found ${vectors.length} vectors in namespace: ${namespace || 'default'}`,
      );

      if (vectors.length === 0) continue;

      // Prepare records for upsert
      const records = vectors.map((match) => ({
        id: match.id,
        values: match.values || [],
        metadata: match.metadata || {},
      }));

      // Upsert in smaller batches
      const upsertBatchSize = 100;
      for (let i = 0; i < records.length; i += upsertBatchSize) {
        const batch = records.slice(i, i + upsertBatchSize);

        if (namespace) {
          await targetIndex.namespace(namespace).upsert(batch);
        } else {
          await targetIndex.upsert(batch);
        }

        copiedCount += batch.length;

        console.log(`Copied ${copiedCount}/${totalVectors} vectors...`);
      }
    }

    console.log(
      `Successfully copied ${copiedCount} vectors from baml-index to baml-index-sage`,
    );

    // Verify the copy
    const targetStats = await targetIndex.describeIndexStats();
    console.log('Target index stats after copy:', targetStats);
  } catch (error) {
    console.error('Error copying Pinecone index:', error);
    throw error;
  }
}
