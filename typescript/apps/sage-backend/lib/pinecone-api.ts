import { Pinecone } from '@pinecone-database/pinecone';
import { chunk } from 'lodash';
import {
  EMBEDDING_MODEL,
  CorpusDocumentSchema,
  embedDocs,
  loadCorpusDocs,
  openai,
} from './corpus';

export type { CorpusDocument, EmbeddingWithMetadata } from './corpus';
export { estimateTokens, chunkMarkdown } from './corpus';

const INDEX_NAME =
  process.env.PINECONE_ENV === 'prod' || process.env.PINECONE_ENV === 'production'
    ? 'ask-baml-prod'
    : 'ask-baml-dev';
console.log('Using Pinecone index:', INDEX_NAME);

const pineconeIndex = new Pinecone({ apiKey: process.env.PINECONE_API_KEY ?? '' }).Index(INDEX_NAME);

export async function populatePinecone(docsYmlPath: string): Promise<void> {
  const docs = await loadCorpusDocs(docsYmlPath);
  const embeddings = await embedDocs(docs);
  console.log(`Computed embeddings for ${embeddings.length} chunks`);

  console.log('Before stats', await pineconeIndex.describeIndexStats());
  console.log('Deleted old embeddings', await pineconeIndex.deleteAll());

  const records = embeddings.map(({ embedding, document }) => ({
    id: `${document.url}::chunk-${document.chunkIndex}`,
    values: embedding,
    metadata: document,
  }));
  const batches = chunk(records, 100);
  console.log(`Upserting ${records.length} records in ${batches.length} batches`);
  await Promise.all(
    batches.map((batch, i) =>
      pineconeIndex.upsert(batch).then(() => console.log(`Batch ${i + 1}/${batches.length}`)),
    ),
  );

  console.log('After stats', await pineconeIndex.describeIndexStats());
  console.log(`✅ Successfully populated Pinecone with ${embeddings.length} chunks`);
}

export async function searchPinecone(query: string) {
  const { data } = await openai.embeddings.create({ model: EMBEDDING_MODEL, input: query });
  const results = await pineconeIndex.query({ vector: data[0].embedding, topK: 7, includeMetadata: true });
  console.info(`Found ${results.matches.length} matches in Pinecone for query`);
  return results.matches.map((m) => CorpusDocumentSchema.parse(m.metadata));
}
