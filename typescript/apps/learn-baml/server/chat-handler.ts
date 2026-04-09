import OpenAI from 'openai';
import { b } from '../baml_client/baml_client';
import type { DocContext, Message } from '../baml_client/baml_client';
import { ensureEmbeddingsFresh, loadEmbeddingsIndex, EmbeddingsIndex } from './embeddings';

interface ChatRequest {
  message: string;
  prev_messages?: Array<{ role: 'user' | 'assistant'; text: string }>;
  stream?: boolean;
}

interface SearchResult {
  id: string;
  title: string;
  url: string;
  content: string;
  section?: string;
  score: number;
}

function jsonResponse(data: unknown, status = 200): Response {
  return new Response(JSON.stringify(data), {
    status,
    headers: { 'Content-Type': 'application/json' },
  });
}

function cosineSimilarity(a: number[], b: number[]): number {
  if (a.length !== b.length) {
    throw new Error(`Vector length mismatch: ${a.length} vs ${b.length}`);
  }

  let dotProduct = 0;
  let normA = 0;
  let normB = 0;

  for (let i = 0; i < a.length; i++) {
    dotProduct += a[i] * b[i];
    normA += a[i] * a[i];
    normB += b[i] * b[i];
  }

  const denominator = Math.sqrt(normA) * Math.sqrt(normB);
  if (denominator === 0) return 0;

  return dotProduct / denominator;
}

function searchByEmbedding(
  queryEmbedding: number[],
  index: EmbeddingsIndex,
  topK = 5
): SearchResult[] {
  const scored = index.chunks.map(chunk => ({
    ...chunk,
    score: cosineSimilarity(queryEmbedding, chunk.embedding),
  }));

  scored.sort((a, b) => b.score - a.score);

  return scored.slice(0, topK).map(({ embedding, ...rest }) => rest);
}

async function embedQuery(
  query: string,
  openai: OpenAI,
  model = 'text-embedding-3-small'
): Promise<number[]> {
  const response = await openai.embeddings.create({
    model,
    input: query,
  });
  if (!response.data || response.data.length === 0) {
    throw new Error('No embedding returned from OpenAI');
  }
  return response.data[0].embedding;
}

function buildSseHeaders(): HeadersInit {
  return {
    'Content-Type': 'text/event-stream',
    'Cache-Control': 'no-cache',
    Connection: 'keep-alive',
    'X-Accel-Buffering': 'no',
  };
}

export async function handleChatRequest(req: Request): Promise<Response> {
  if (req.method !== 'POST') {
    return new Response('Method not allowed', { status: 405 });
  }

  const freshness = ensureEmbeddingsFresh();
  if (!freshness.ok) {
    return jsonResponse({ error: freshness.message }, 503);
  }

  let payload: ChatRequest;
  try {
    payload = (await req.json()) as ChatRequest;
  } catch {
    return jsonResponse({ error: 'Invalid JSON body' }, 400);
  }

  const { message, prev_messages = [], stream = false } = payload ?? {};

  if (!message?.trim()) {
    return jsonResponse({ error: 'Message is required' }, 400);
  }

  if (message.length > 2000) {
    return jsonResponse({ error: 'Message too long (max 2000 characters)' }, 400);
  }

  if (!Array.isArray(prev_messages)) {
    return jsonResponse({ error: 'prev_messages must be an array' }, 400);
  }

  if (prev_messages.length > 20) {
    return jsonResponse({ error: 'Too many previous messages (max 20)' }, 400);
  }

  if (!process.env.OPENAI_API_KEY) {
    return jsonResponse({ error: 'Service temporarily unavailable' }, 503);
  }

  try {
    const openai = new OpenAI({ apiKey: process.env.OPENAI_API_KEY });
    const index = loadEmbeddingsIndex();
    const queryEmbedding = await embedQuery(message, openai, index.model);
    const relevantDocs = searchByEmbedding(queryEmbedding, index, 5);

    const contextDocs: DocContext[] = relevantDocs.map(doc => ({
      title: doc.title,
      url: doc.url,
      content: doc.content,
      section: doc.section ?? null,
    }));

    const prevMessages: Message[] = prev_messages.map(m => ({
      role: m.role,
      text: m.text,
    }));

    const bamlInput = {
      query: message,
      context_docs: contextDocs,
      prev_messages: prevMessages,
    };

    if (!stream) {
      const result = await b.AskBaml(bamlInput);
      return jsonResponse({
        answer: result.answer,
        citations: result.citations.map(c => ({
          title: c.title,
          url: c.url,
          relevance: c.relevance,
        })),
        suggested_questions: result.suggested_questions ?? [],
        _debug: {
          doc_scores: relevantDocs.map(d => d.score),
        },
      });
    }

    const encoder = new TextEncoder();
    let aborted = false;

    req.signal.addEventListener('abort', () => {
      aborted = true;
    });

    const streamResponse = new ReadableStream({
      async start(controller) {
        const writeEvent = (data: unknown) => {
          controller.enqueue(encoder.encode(`data: ${JSON.stringify(data)}\n\n`));
        };

        try {
          writeEvent({ type: 'doc_scores', scores: relevantDocs.map(d => d.score) });

          const bamlStream = b.stream.AskBaml(bamlInput);

          for await (const partial of bamlStream) {
            if (aborted) break;
            writeEvent({
              type: 'partial',
              answer: partial.answer ?? '',
              citations: (partial.citations ?? []).map(c => ({
                title: c?.title ?? '',
                url: c?.url ?? '',
                relevance: c?.relevance ?? 'Low',
              })),
              suggested_questions: partial.suggested_questions ?? [],
            });
          }

          if (!aborted) {
            const final = await bamlStream.getFinalResponse();
            writeEvent({
              type: 'done',
              answer: final.answer,
              citations: final.citations.map(c => ({
                title: c.title,
                url: c.url,
                relevance: c.relevance,
              })),
              suggested_questions: final.suggested_questions ?? [],
            });
          }
        } catch (error) {
          console.error('Chat error:', error);
          writeEvent({ type: 'error', error: 'Failed to process request' });
        } finally {
          controller.close();
        }
      },
    });

    return new Response(streamResponse, { headers: buildSseHeaders() });
  } catch (error) {
    console.error('Chat error:', error);
    return jsonResponse({ error: 'Failed to process request' }, 500);
  }
}
