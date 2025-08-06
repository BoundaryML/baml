'use server';

import { z } from 'zod';
import { b } from '../../baml_client';
import type { QueryRequest } from '../types';
import { searchPinecone } from './rag';

// Define the Zod schema for the response
const QueryResponseSchema = z.object({
  ranked_docs: z.array(
    z.object({
      title: z.string(),
      url: z.string(),
      relevance: z.enum(['very-relevant', 'relevant', 'not-relevant']),
    }),
  ),
  answer: z.string().optional().or(z.null()),
  suggested_messages: z.array(z.string()).optional(),
});

export type QueryResponse = z.infer<typeof QueryResponseSchema>;

export async function submitQuery(
  queryRequest: QueryRequest,
): Promise<QueryResponse> {
  const docs = await searchPinecone(queryRequest.query);
  const pineconeRankedDocs = docs.map((doc) => ({
    title: (doc.metadata?.title ?? '') as string,
    url: (doc.metadata?.slug ?? '') as string,
    body: (doc.metadata?.body ?? '') as string,
  }));

  const plan = await b.PlanQuery({
    text: queryRequest.query,
    language_preference: queryRequest.language_preference,
    context_docs: pineconeRankedDocs.map((doc) => ({
      title: doc.title,
      body: doc.body,
    })),
    prev_messages: queryRequest.prev_messages,
  });

  // Merge titles from rankedDocs into plan.ranked_docs
  const relevantDocs = (plan.ranked_docs ?? []).map((planDoc) => {
    const matchingRankedDoc = pineconeRankedDocs.find(
      (rd) => rd.title === planDoc.title,
    );
    return {
      title: planDoc.title,
      url: matchingRankedDoc?.url ?? '',
      relevance: planDoc.relevance,
    };
  });

  const resp = {
    answer: plan.answer,
    ranked_docs: Array.from(
      new Map(relevantDocs.map((doc) => [doc.url, doc])).values(),
    ),
    suggested_messages: plan.refine_query?.suggested_queries,
  };

  return resp;
}
