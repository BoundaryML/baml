import { z } from 'zod';

export const UserMessageSchema = z.object({
  role: z.literal('user'),
  language_preference: z.string().optional(),
  text: z.string(),
});

export type UserMessage = z.infer<typeof UserMessageSchema>;

export const AssistantMessageSchema = z.object({
  role: z.literal('assistant'),
  message_id: z.string(), // ISO8601 timestamp
  ranked_docs: z.array(
    z.object({
      title: z.string(),
      url: z.string(),
      relevance: z.enum(['very-relevant', 'relevant', 'not-relevant']),
    }),
  ),
  text: z.string().optional().or(z.null()),
  suggested_messages: z.array(z.string()).optional(),
});

export type AssistantMessage = z.infer<typeof AssistantMessageSchema>;

export const MessageSchema = z.discriminatedUnion('role', [
  UserMessageSchema,
  AssistantMessageSchema,
]);

export type Message = z.infer<typeof MessageSchema>;

/**
 * Schema for requests to the doc-chat API
 */
export const QueryRequestSchema = z.object({
  session_id: z.string(),
  prev_messages: z.array(MessageSchema),
  message: UserMessageSchema,
});

export type QueryRequest = z.infer<typeof QueryRequestSchema>;

/**
 * Schema for responses from the doc-chat API
 */
export const QueryResponseSchema = z.object({
  session_id: z.string(),
  message: AssistantMessageSchema,
});

export type QueryResponse = z.infer<typeof QueryResponseSchema>;

export const SendFeedbackRequestSchema = z.object({
  session_id: z.string(),
  // contents that will be sent to slack/notion
  messages: z.array(MessageSchema),
  feedback_type: z.enum(['thumbs_up', 'thumbs_down']),
  comment: z.string().max(1000).optional(),
});
export type SendFeedbackRequest = z.infer<typeof SendFeedbackRequestSchema>;

export const SendFeedbackResponseSchema = z.object({
  enqueued: z.boolean(),
});

export type SendFeedbackResponse = z.infer<typeof SendFeedbackResponseSchema>;
