import OpenAI from 'openai';

export interface SearchResult {
  id: string;
  title: string;
  url: string;
  content: string;
  section?: string;
  score: number;
}

interface EmbeddingsIndex {
  model: string;
  dimensions: number;
  chunks: Array<{
    id: string;
    title: string;
    url: string;
    content: string;
    section?: string;
    embedding: number[];
  }>;
}

let cachedIndex: EmbeddingsIndex | null = null;

/**
 * Load embeddings index (cached after first load)
 */
export async function loadEmbeddingsIndex(): Promise<EmbeddingsIndex> {
  if (cachedIndex) return cachedIndex;

  // In browser, fetch from static file
  // In server, could also import directly
  const response = await fetch('/embeddings.json');
  cachedIndex = await response.json();
  return cachedIndex!;
}

/**
 * Load embeddings index on server side
 */
export async function loadEmbeddingsIndexServer(baseUrl: string): Promise<EmbeddingsIndex> {
  if (cachedIndex) return cachedIndex;

  const response = await fetch(`${baseUrl}/embeddings.json`);
  cachedIndex = await response.json();
  return cachedIndex!;
}

/**
 * Compute cosine similarity between two vectors
 */
function cosineSimilarity(a: number[], b: number[]): number {
  let dotProduct = 0;
  let normA = 0;
  let normB = 0;

  for (let i = 0; i < a.length; i++) {
    dotProduct += a[i] * b[i];
    normA += a[i] * a[i];
    normB += b[i] * b[i];
  }

  return dotProduct / (Math.sqrt(normA) * Math.sqrt(normB));
}

/**
 * Search for relevant documents given a query embedding
 */
export function searchByEmbedding(
  queryEmbedding: number[],
  index: EmbeddingsIndex,
  topK = 5
): SearchResult[] {
  const scored = index.chunks.map(chunk => ({
    ...chunk,
    score: cosineSimilarity(queryEmbedding, chunk.embedding),
  }));

  // Sort by score descending
  scored.sort((a, b) => b.score - a.score);

  // Return top K without embedding (no need to send to client)
  return scored.slice(0, topK).map(({ embedding, ...rest }) => rest);
}

/**
 * Embed a query using OpenAI
 */
export async function embedQuery(
  query: string,
  openai: OpenAI,
  model = 'text-embedding-3-small'
): Promise<number[]> {
  const response = await openai.embeddings.create({
    model,
    input: query,
  });
  return response.data[0].embedding;
}
