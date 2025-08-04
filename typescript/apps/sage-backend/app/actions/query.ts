'use server';

import { b } from '../../baml_client';
import type { QueryRequest, QueryResponse } from '@baml/sage-interface';
import { searchPinecone } from './rag';

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
