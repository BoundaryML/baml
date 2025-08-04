import { z } from 'zod';

/**
 * Schema for requests to the doc-chat API
 */
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

/**
 * Schema for responses from the doc-chat API
 */
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

/**
 * Schema for send-to-slack API requests
 */
export const SendToSlackRequestSchema = z.object({
  question: z.string().min(1).max(2000),
  answer: z.string().min(1).max(8000), 
  ranked_docs: z.array(z.object({
    title: z.string(),
    url: z.string(),
    relevance: z.enum(['very-relevant', 'relevant', 'not-relevant'])
  })).optional(),
  channel: z.string().regex(/^#[a-z0-9-_]+$/i).default('#general'),
  user_email: z.string().email().optional()
});

export type SendToSlackRequest = z.infer<typeof SendToSlackRequestSchema>;

/**
 * Schema for send-to-slack API responses
 */
export const SendToSlackResponseSchema = z.object({
  success: z.boolean(),
  message_ts: z.string().optional(),
  channel: z.string().optional(), 
  permalink: z.string().optional(),
  error: z.string().optional(),
  code: z.enum(['INVALID_CHANNEL', 'RATE_LIMITED', 'SLACK_ERROR', 'VALIDATION_ERROR']).optional()
});

export type SendToSlackResponse = z.infer<typeof SendToSlackResponseSchema>;