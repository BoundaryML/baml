import { describe, it } from 'vitest';
import { populatePinecone } from './pinecone-api';

describe('pinecone-api', () => {
  it('should call populatePinecone', async () => {
    await populatePinecone('/Users/sam/baml2/fern/docs.yml');
  });
}, 60_000);
