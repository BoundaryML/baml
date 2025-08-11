import { z } from 'zod';
// IfChange
export const QueryRequestSchema = z.object({
  query: z.string(),
  language_preference: z.string().optional(),
  prev_messages: z.array(
    z.object({
      role: z.enum(['user', 'assistant']),
      text: z.string(),
    }),
  ),
});
export type QueryRequest = z.infer<typeof QueryRequestSchema>;

export const QueryResponseSchema = z.object({
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
// ThenChange baml/sage-backend/app/types.ts
