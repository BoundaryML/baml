import { z } from "zod";

export const eventIOTypeSchema = z.object({
  name: z.string(),
  fields: z.any(),
});

export const llmOutputSchema = z.object({
  raw_text: z.string(),
  metadata: z.object({
    logprobs: z.any().optional().nullable(),
    // TODO: Consider if the backend can populate these fields.
    prompt_tokens: z.number().int().optional().nullable(),
    output_tokens: z.number().int().optional().nullable(),
    total_tokens: z.number().int().optional().nullable(),
    finish_reason: z.string().optional().nullable(),
  }),
  override: z.any().optional().nullable(),
});

export const llmChatSchema = z.object({
  role: z.string(),
  content: z.string(),
});

export const llmEventSchema = z.object({
  model_name: z.string(),
  provider: z.string(),
  input: z.object({
    prompt: z.object({
      template: z.union([z.string(), z.array(llmChatSchema)]),
      template_args: z.record(z.string()).default({}),
      override: z.any().optional().nullable(),
    }),
    invocation_params: z.record(z.any()),
  }),
  output: llmOutputSchema.optional().nullable(),
});

export enum EventType {
  log = "log",
  llm = "func_llm",
  model = "func_prob",
  code = "func_code",
}

export const clientEventLogSchema = z.object({
  project_id: z.string(),
  root_event_id: z.string().uuid(),
  event_id: z.string().uuid(),
  event_type: z.nativeEnum(EventType),
  parent_event_id: z.string().uuid().optional().nullable(),
  context: z.object({
    hostname: z.string(),
    process_id: z.string(),
    stage: z.string().default("prod"),
    start_time: z.string().datetime(),
    latency_ms: z.number().default(0),
    tags: z.record(z.string()).optional().nullable(),
    event_chain: z
      .array(
        z.object({
          function_name: z.string(),
          variant_name: z.string().optional().nullable(),
        })
      )
      .min(1),
  }),
  io: z.object({
    input: z
      .object({
        value: z.any(),
        type: eventIOTypeSchema,
        override: z.any().optional().nullable(),
      })
      .optional()
      .nullable(),
    output: z
      .object({
        value: z.any(),
        type: eventIOTypeSchema,
        override: z.any().optional().nullable(),
      })
      .optional()
      .nullable(),
  }),
  error: z
    .object({
      code: z.number().int(),
      message: z.string(),
      traceback: z.string().optional().nullable(),
      override: z.any().optional().nullable(),
    })
    .optional()
    .nullable(),
  metadata: llmEventSchema.optional().nullable(),
});

export type ClientEventLog = z.infer<typeof clientEventLogSchema>;
export type LLMEvent = z.infer<typeof llmEventSchema>;
export type LLMOutput = z.infer<typeof llmOutputSchema>;
export type LLMChat = z.infer<typeof llmChatSchema>;