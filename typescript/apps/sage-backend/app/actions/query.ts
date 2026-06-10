'use server';

import type { QueryRequest, QueryResponse } from '@baml/sage-interface';
import { b } from '../../baml_client';
import { searchPinecone } from '../../lib/pinecone-api';
import { searchQdrant } from '../../lib/qdrant-api';

const searchDocs = process.env.VECTOR_DB === 'qdrant' ? searchQdrant : searchPinecone;

export async function submitQuery(request: QueryRequest): Promise<QueryResponse> {
  const contextDocs = await searchDocs(request.message.text);

  const plan = await b.PlanQuery({
    text: request.message.text,
    language_preference: request.message.language_preference,
    context_docs: contextDocs.map(({ title, body }) => ({ title, body })),
    prev_messages: request.prev_messages.map((msg) => {
      if (msg.role === 'assistant') {
        return { role: 'assistant', text: msg.text ?? '' };
      }
      return msg;
    }),
  });

  const relevantDocs = (plan.ranked_docs ?? []).map((planDoc) => ({
    title: planDoc.title,
    url: contextDocs.find((d) => d.title === planDoc.title)?.url ?? '',
    relevance: planDoc.relevance,
  }));

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
