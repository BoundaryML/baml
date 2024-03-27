import { Completion } from "openai/resources";
import { Stream } from "openai/streaming";
import { ChatCompletion, ChatCompletionChunk, ChatCompletionCreateParams } from "openai/resources/chat/completions";
import { clientManager } from "../client_manager";
import { APIError, OpenAI, OpenAIError } from "openai";
import { LLMChatProvider } from "../llm_chat_provider";
import { LLMChatMessage, LLMResponse, LLMBaseProviderArgs } from "../llm_base_provider";

class OpenAIClient extends LLMChatProvider {
    private client: OpenAI;
    private params: Omit<ChatCompletionCreateParams, 'messages'>;

    constructor(params: LLMBaseProviderArgs) {
        const {
            api_key,
            api_base,
            api_version,
            api_type,
            engine,
            organization,
            base_url,
            timeout,
            max_retries,
            model,
            frequency_penalty,
            logit_bias,
            logprobs,
            max_tokens,
            presence_penalty,
            response_format,
            seed,
            stop,
            temperature,
            top_p,
            user,
            ...rest
        } = params;

        super({
            prompt_to_chat: (prompt) => {
                return {
                    role: 'user',
                    content: prompt,
                }
            },
            ...rest
        });

        if (api_type === "azure") {
            throw new Error("Azure API is not supported. Use `baml-azure-chat` instead");
        } else {
            this.client = new OpenAI({
                apiKey: api_key,
                organization: organization,
                baseURL: base_url,
                timeout: timeout,
                maxRetries: max_retries ?? 0,
            });
        }

        this.params = {
            model,
            frequency_penalty,
            logit_bias,
            logprobs,
            max_tokens,
            presence_penalty,
            response_format,
            seed,
            stop,
            temperature: temperature ?? 0,
            top_p,
            user,
        }
    }

    protected to_error_code_impl(err: unknown): number | undefined {
        if (err instanceof OpenAIError) {
            if (err instanceof APIError) {
                return err.status;
            }
        }
        return undefined;
    }

    protected async chat_impl(prompt: LLMChatMessage[]): Promise<LLMResponse> {
        const response = await this.client.chat.completions.create({
            messages: prompt.map((chat) => ({
                role: chat.role as 'user',
                content: chat.content,
            })),
            ...this.params,
            n: 1,
            stream: false,
        });

        const choice = response.choices[0];
        if (choice === undefined || choice === null) {
            throw new Error("Choice is undefined");
        }

        const message = choice.message.content;
        if (message === undefined || message === null) {
            throw new Error("Message is undefined");
        }

        return {
            generated: message,
            model_name: response.model,
            meta: {
                finish_reason: choice.finish_reason,
                prompt_tokens: response.usage?.prompt_tokens,
                output_tokens: response.usage?.completion_tokens,
                total_tokens: response.usage?.total_tokens,
            }
        }
    }

    protected stream_impl(prompts: LLMChatMessage[]): AsyncIterable<LLMResponse> {
        const stream = this.client.chat.completions.create({
            messages: prompts.map(({role, content}) => ({
                role: role === "user" || role === "assistant" ? role : "system",
                content,
            })),
            stream: true,
            ...this.params,
        });
        return {
            async *[Symbol.asyncIterator](): AsyncIterableIterator<LLMResponse> {
                let last_chunk = null;
                let finish_reason: string | null = null;

                for await (const chunk of (await stream as Stream<ChatCompletionChunk>)) {
                    last_chunk = chunk;

                    finish_reason = chunk.choices[0]?.finish_reason || null;

                    yield {
                        generated: chunk.choices[0]?.delta?.content || '',
                        model_name: chunk.model ||  "<unknown-stream-model>",
                        meta: {
                            baml_is_complete: last_chunk.choices[0]?.finish_reason === "stop",
                            finish_reason,
                            // NB: OpenAI does not currently provide token usage data for streams
                            prompt_tokens: null,
                            output_tokens: null,
                            total_tokens: null,
                        },
                    };
                }
            }
        };
    }
}

clientManager.registerProvider("baml-openai-chat", {
    createClient: (name: string, options: LLMBaseProviderArgs): LLMChatProvider => {
        return new OpenAIClient(options);
    },
});
