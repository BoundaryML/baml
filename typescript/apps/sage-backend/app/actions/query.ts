'use server';

import type { QueryRequest, QueryResponse } from '@baml/sage-interface';
import { searchPinecone } from '../../lib/pinecone-api';

async function tryLoadBamlClient(): Promise<any | null> {
  try {
    // Avoid static analysis by Next/Webpack
    // eslint-disable-next-line no-eval
    const mod = await (0, eval)("import('../../' + 'baml_client')");
    return mod;
  } catch {
    return null;
  }
}

export async function submitQuery(request: QueryRequest): Promise<QueryResponse> {
  const docs = await searchPinecone(request.message.text);
  const pineconeRankedDocs = docs.map((doc) => ({
    title: doc.title,
    url: doc.url,
    body: doc.body,
  }));

  const baml = await tryLoadBamlClient();

  if (!baml) {
    // Fallback: return top docs summary without LLM plan
    const ranked: Array<{ title: string; url: string; relevance: 'very-relevant' | 'relevant' | 'not-relevant' }> =
      pineconeRankedDocs.slice(0, 3).map((d) => ({
        title: d.title,
        url: d.url,
        relevance: 'very-relevant',
      }));

    return {
      session_id: request.session_id,
      message: {
        role: 'assistant',
        message_id: `msg-${new Date().toISOString()}`,
        text:
          'BAML client unavailable at build time. Returning top related docs. Ask again once the backend is fully configured.',
        ranked_docs: ranked,
        suggested_messages: [],
      },
    };
  }

  const { b } = baml as { b: any };

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
  const relevantDocs: Array<{ title: string; url: string; relevance: 'very-relevant' | 'relevant' | 'not-relevant' }> =
    (plan.ranked_docs ?? []).map((planDoc: any) => {
      const matchingRankedDoc = pineconeRankedDocs.find((rd) => rd.title === planDoc.title);
      return {
        title: planDoc.title,
        url: matchingRankedDoc?.url ?? '',
        relevance: planDoc.relevance,
      } as { title: string; url: string; relevance: 'very-relevant' | 'relevant' | 'not-relevant' };
    });

  const dedupedRankedDocs: Array<{ title: string; url: string; relevance: 'very-relevant' | 'relevant' | 'not-relevant' }> = Array.from(
    new Map(relevantDocs.map((doc) => [doc.url, doc])).values(),
  );

  return {
    session_id: request.session_id,
    message: {
      role: 'assistant',
      message_id: `msg-${new Date().toISOString()}`,
      text: plan.answer,
      ranked_docs: dedupedRankedDocs,
      suggested_messages: plan.refine_query?.suggested_queries,
    },
  };
}
