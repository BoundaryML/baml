import { Pinecone } from '@pinecone-database/pinecone';
import OpenAI from 'openai';
import {
  loadSitemap,
  getInternalDocs,
  getExternalBlogs,
  processInternalDocs,
  processExternalBlogs,
  type FernDoc,
} from './sitemap';

const openaiClient = new OpenAI({
  apiKey: process.env.OPENAI_API_KEY ?? '',
});

const pineconeClient = new Pinecone({
  apiKey: process.env.PINECONE_API_KEY ?? '',
});

const pineconeIndex = pineconeClient.Index('baml-index-sage');

export interface EmbeddingWithMetadata {
  embedding: number[];
  document: FernDoc;
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

/**
 * Generate embeddings for text chunks and prepare for Pinecone upsert
 */
async function generateEmbeddingsForDocs(
  docs: FernDoc[],
): Promise<EmbeddingWithMetadata[]> {
  const chunkedDocs: { doc: FernDoc; chunk: string; chunkIndex: number }[] =
    docs.flatMap((doc: FernDoc) => {
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

  return embeddingsWithMetadata;
}

/**
 * Upsert embeddings to Pinecone in batches
 */
async function upsertToPinecone(embeddingsWithMetadata: EmbeddingWithMetadata[]): Promise<void> {
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

/**
 * Main function to populate Pinecone with documents from sitemap
 */
export async function populatePinecone(): Promise<void> {
  // Read sitemap.json which contains all documentation sources
  const sitemap = loadSitemap();
  console.log(`Found ${sitemap.length} total entries in sitemap`);

  // Separate internal docs and external blog posts
  const internalDocs = getInternalDocs(sitemap);
  const externalBlogs = getExternalBlogs(sitemap);

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
  const internalFernDocs = processInternalDocs(internalDocs);

  // Process external blog posts
  const externalFernDocs = await processExternalBlogs(externalBlogs);

  // Combine all documents
  const allDocs = [...internalFernDocs, ...externalFernDocs];
  console.log(`Total documents to process: ${allDocs.length}`);

  // Process docs in batches of 10
  for (let i = 0; i < allDocs.length; i += 10) {
    const batch = allDocs.slice(i, i + 10);
    console.log(
      `Processing batch ${Math.floor(i / 10) + 1}/${Math.ceil(allDocs.length / 10)}`,
    );

    // Generate embeddings for the batch
    const embeddingsWithMetadata = await generateEmbeddingsForDocs(batch);

    // Upsert to Pinecone
    await upsertToPinecone(embeddingsWithMetadata);
  }

  console.log(
    `✅ Successfully populated Pinecone with ${allDocs.length} documents`,
  );
}

/**
 * Test function to verify populate works with a small subset
 */
export async function testPopulatePinecone(): Promise<boolean> {
  try {
    // Read sitemap and take a small sample
    const sitemap = loadSitemap();

    const sampleInternal = getInternalDocs(sitemap).slice(0, 2);
    const sampleExternal = getExternalBlogs(sitemap).slice(0, 1);

    console.log(
      `Testing with ${sampleInternal.length} internal docs and ${sampleExternal.length} external blogs`,
    );

    // Test internal doc processing
    const internalFernDocs = processInternalDocs(sampleInternal);
    for (const doc of internalFernDocs) {
      console.log(`✓ Internal doc "${doc.title}": ${doc.body.length} characters`);
    }

    // Test external blog processing
    const externalFernDocs = await processExternalBlogs(sampleExternal);
    for (const doc of externalFernDocs) {
      console.log(`✓ External blog "${doc.title}": ${doc.body.length} characters`);
      console.log(doc.body.slice(0, 200));
    }

    console.log('✅ Test completed successfully');
    return true;
  } catch (error) {
    console.error('❌ Test failed:', error);
    return false;
  }
}

/**
 * Search Pinecone for relevant documents using vector similarity
 */
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
  return results.matches;
}

/**
 * Copy vectors from baml-index to baml-index-sage (utility function)
 */
export async function copyPineconeIndex(): Promise<void> {
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

    // We need to query in batches since Pinecone doesn't have a "list all" operation
    const batchSize = 1000;
    let copiedCount = 0;

    // Get all unique namespaces first
    const namespaces = Object.keys(stats.namespaces || { '': stats });

    for (const namespace of namespaces) {
      console.log(`Processing namespace: ${namespace || 'default'}`);

      // Query with a zero vector to get vectors (this is a workaround)
      const queryOptions = {
        topK: Math.min(batchSize, 10000), // Pinecone max is 10000
        includeMetadata: true,
        includeValues: true,
      };

      // We need to provide a vector for the query, so we'll use a dummy vector
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