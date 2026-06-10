import { QdrantClient } from '@qdrant/js-client-rest';
import { chunk } from 'lodash';
import {
  EMBEDDING_MODEL,
  CorpusDocumentSchema,
  embedDocs,
  loadCorpusDocs,
  openai,
} from './corpus';

const COLLECTION =
  process.env.QDRANT_ENV === 'prod' || process.env.QDRANT_ENV === 'production'
    ? 'ask-baml-prod'
    : 'ask-baml-dev';
console.log('Using Qdrant collection:', COLLECTION);

const qdrant = new QdrantClient({
  url: process.env.QDRANT_URL ?? 'http://localhost:6333',
  apiKey: process.env.QDRANT_API_KEY,
});

const VECTOR_SIZE = 3072;

export async function populateQdrant(docsYmlPath: string): Promise<void> {
  const docs = await loadCorpusDocs(docsYmlPath);
  const embeddings = await embedDocs(docs);
  console.log(`Computed embeddings for ${embeddings.length} chunks`);

  const { exists } = await qdrant.collectionExists(COLLECTION);
  if (exists) await qdrant.deleteCollection(COLLECTION);
  await qdrant.createCollection(COLLECTION, { vectors: { size: VECTOR_SIZE, distance: 'Cosine' } });

  const points = embeddings.map(({ embedding, document }, id) => ({
    id,
    vector: embedding,
    payload: document as Record<string, unknown>,
  }));
  const batches = chunk(points, 100);
  console.log(`Upserting ${points.length} points in ${batches.length} batches`);
  await Promise.all(
    batches.map((batch, i) =>
      qdrant.upsert(COLLECTION, { wait: true, points: batch }).then(() => console.log(`Batch ${i + 1}/${batches.length}`)),
    ),
  );

  const { count } = await qdrant.count(COLLECTION);
  console.log('After stats', { count });
  console.log(`✅ Successfully populated Qdrant with ${embeddings.length} chunks`);
}

export async function searchQdrant(query: string) {
  const { data } = await openai.embeddings.create({ model: EMBEDDING_MODEL, input: query });
  const { points } = await qdrant.query(COLLECTION, { query: data[0].embedding, limit: 7, with_payload: true });
  console.info(`Found ${points.length} matches in Qdrant for query`);
  return points.map((p) => CorpusDocumentSchema.parse(p.payload));
}
