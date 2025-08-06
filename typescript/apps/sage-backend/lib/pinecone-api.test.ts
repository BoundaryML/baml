import { describe, it } from 'vitest';
import { populatePinecone } from './pinecone-api';

describe('pinecone-api', () => {
  it('should call populatePinecone', async () => {
    await populatePinecone();
  });
}, 60_000);
