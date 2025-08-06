'use server';

import type { QueryRequest, QueryResponse } from '@baml/sage-interface';
import { b } from '../../baml_client';
import { searchPinecone } from '../../lib/pinecone-api';

export async function submitQuery(request: QueryRequest): Promise<QueryResponse> {
  const docs = await searchPinecone(request.message.text);
  const pineconeRankedDocs = docs.map((doc) => ({
    title: doc.title,
    url: doc.url,
    body: doc.body,
  }));

  const plan = await b.PlanQuery({
    text: request.message.text,
    language_preference: request.message.language_preference,
    context_docs: pineconeRankedDocs.map((doc) => ({
      title: doc.title,
      body: doc.body,
    })),
    prev_messages: request.prev_messages.map((msg) => {
      if (msg.role === 'assistant') {
        return {
          role: 'assistant',
          text: msg.text ?? '',
        };
      }
      return msg;
    }),
  });

  // Merge titles from rankedDocs into plan.ranked_docs
  const relevantDocs = (plan.ranked_docs ?? []).map((planDoc) => {
    const matchingRankedDoc = pineconeRankedDocs.find((rd) => rd.title === planDoc.title);
    return {
      title: planDoc.title,
      url: matchingRankedDoc?.url ?? '',
      relevance: planDoc.relevance,
    };
  });

  return {
    session_id: request.session_id,
    message: {
      role: 'assistant',
      message_id: `msg-${new Date().toISOString()}`,
      text: plan.answer,
      ranked_docs: Array.from(new Map(relevantDocs.map((doc) => [doc.url, doc])).values()),
      suggested_messages: plan.refine_query?.suggested_queries,
    },
  };
}
