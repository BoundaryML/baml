"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.clientEventLogSchema = exports.EventType = exports.llmEventSchema = exports.llmChatSchema = exports.llmOutputSchema = exports.eventIOTypeSchema = void 0;
var zod_1 = require("zod");
exports.eventIOTypeSchema = zod_1.z.object({
    name: zod_1.z.string(),
    fields: zod_1.z.any(),
});
exports.llmOutputSchema = zod_1.z.object({
    raw_text: zod_1.z.string(),
    metadata: zod_1.z.object({
        logprobs: zod_1.z.any().optional().nullable(),
        // TODO: Consider if the backend can populate these fields.
        prompt_tokens: zod_1.z.number().int().optional().nullable(),
        output_tokens: zod_1.z.number().int().optional().nullable(),
        total_tokens: zod_1.z.number().int().optional().nullable(),
        finish_reason: zod_1.z.string().optional().nullable(),
    }),
    override: zod_1.z.any().optional().nullable(),
});
exports.llmChatSchema = zod_1.z.object({
    role: zod_1.z.string(),
    content: zod_1.z.string(),
});
exports.llmEventSchema = zod_1.z.object({
    model_name: zod_1.z.string(),
    provider: zod_1.z.string(),
    input: zod_1.z.object({
        prompt: zod_1.z.object({
            template: zod_1.z.union([zod_1.z.string(), zod_1.z.array(exports.llmChatSchema)]),
            template_args: zod_1.z.record(zod_1.z.string()).default({}),
            override: zod_1.z.any().optional().nullable(),
        }),
        invocation_params: zod_1.z.record(zod_1.z.any()),
    }),
    output: exports.llmOutputSchema.optional().nullable(),
});
var EventType;
(function (EventType) {
    EventType["log"] = "log";
    EventType["llm"] = "func_llm";
    EventType["model"] = "func_prob";
    EventType["code"] = "func_code";
})(EventType || (exports.EventType = EventType = {}));
exports.clientEventLogSchema = zod_1.z.object({
    project_id: zod_1.z.string(),
    root_event_id: zod_1.z.string().uuid(),
    event_id: zod_1.z.string().uuid(),
    event_type: zod_1.z.nativeEnum(EventType),
    parent_event_id: zod_1.z.string().uuid().optional().nullable(),
    context: zod_1.z.object({
        hostname: zod_1.z.string(),
        process_id: zod_1.z.string(),
        stage: zod_1.z.string().default("prod"),
        start_time: zod_1.z.string().datetime(),
        latency_ms: zod_1.z.number().default(0),
        tags: zod_1.z.record(zod_1.z.string()).optional().nullable(),
        event_chain: zod_1.z
            .array(zod_1.z.object({
            function_name: zod_1.z.string(),
            variant_name: zod_1.z.string().optional().nullable(),
        }))
            .min(1),
    }),
    io: zod_1.z.object({
        input: zod_1.z
            .object({
            value: zod_1.z.any(),
            type: exports.eventIOTypeSchema,
            override: zod_1.z.any().optional().nullable(),
        })
            .optional()
            .nullable(),
        output: zod_1.z
            .object({
            value: zod_1.z.any(),
            type: exports.eventIOTypeSchema,
            override: zod_1.z.any().optional().nullable(),
        })
            .optional()
            .nullable(),
    }),
    error: zod_1.z
        .object({
        code: zod_1.z.number().int(),
        message: zod_1.z.string(),
        traceback: zod_1.z.string().optional().nullable(),
        override: zod_1.z.any().optional().nullable(),
    })
        .optional()
        .nullable(),
    metadata: exports.llmEventSchema.optional().nullable(),
});
