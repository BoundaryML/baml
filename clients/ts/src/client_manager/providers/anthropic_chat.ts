import Anthropic, { APIError, AnthropicError } from '@anthropic-ai/sdk';
import { clientManager } from "../client_manager";
import { MessageCreateParamsNonStreaming } from '@anthropic-ai/sdk/resources/messages';
import { LLMChatProvider, LLMChatProviderArgs } from '../llm_chat_provider';
import { LLMBaseProvider, LLMBaseProviderArgs, LLMChatMessage, LLMResponse } from '../llm_base_provider';


class AnthropicClient extends LLMChatProvider {
    private client: Anthropic;
    private params: Omit<MessageCreateParamsNonStreaming, 'messages'>;

    constructor(params: LLMBaseProviderArgs) {
        const {
            api_key,
            base_url,
            timeout,
            max_retries,
            model,
            max_tokens,
            stop_sequences,
            temperature,
            top_k,
            top_p,
            metadata,
            ...rest
        } = params;


        super({
            prompt_to_chat: (prompt: string) => ({
                role: 'user',
                content: prompt,
            }),
            ...rest,
        });
        this.client = new Anthropic({
            apiKey: api_key,
            baseURL: base_url,
            timeout,
            maxRetries: max_retries ?? 0,
        });

        this.params = {
            model,
            max_tokens: max_tokens ?? 1000,
            stop_sequences,
            temperature: temperature ?? 0,
            top_k,
            top_p,
            metadata,
        }
    }

    protected to_error_code_impl(err: unknown): number | undefined {
        if (err instanceof AnthropicError) {
            if (err instanceof APIError) {
                return err.status;
            }
        }

        return undefined;
    }

    protected async chat_impl(prompt: LLMChatMessage[]): Promise<LLMResponse> {

        const systemMessages = prompt.filter(chat => chat.role === "system");
        const nonSystemMessages = prompt.filter(chat => chat.role !== "system");

        if (systemMessages.length > 1) {
            throw new Error("More than one system message found");
        }

        let systemMessage: LLMChatMessage | undefined;
        if (systemMessages.length === 1) {
            systemMessage = systemMessages[0];
        }

        const response = await this.client.messages.create({
            messages: nonSystemMessages.map((chat) => ({
                role: chat.role === "user" ? "user" : "assistant",
                content: chat.content,
            })),
            system: systemMessage?.content,
            ...this.params,
        });

        const message = response.content[0];
        if (message === undefined || message === null) {
            throw new Error("Message is undefined");
        }

        return {
            generated: message.text,
            model_name: response.model,
            meta: {
                finish_reason: response.stop_reason,
            }
        };
    }

    protected stream_impl(prompts: LLMChatMessage[]): AsyncIterable<LLMResponse> {
        const systemMessages = prompts.filter(chat => chat.role === "system");
        const nonSystemMessages = prompts.filter(chat => chat.role !== "system");

        if (systemMessages.length > 1) {
            throw new Error("More than one system message found");
        }

        let systemMessage: LLMChatMessage | undefined;
        if (systemMessages.length === 1) {
            systemMessage = systemMessages[0];
        }

        const stream = this.client.messages.stream({
            messages: nonSystemMessages.map((chat) => ({
                role: chat.role === "user" ? "user" : "assistant",
                content: chat.content,
            })),
            system: systemMessage?.content,
            ...this.params,
        });
        return {
            async *[Symbol.asyncIterator](): AsyncIterableIterator<LLMResponse> {
                let start_message: Anthropic.Message | null = null;
                let last_event = null;
                let output_tokens: number = 0;
                let stop_reason: string | null = null;

                for await (const event of stream) {
                    last_event = event;
                    const {
                        model: model_name = "<unknown-stream-model>"
                    } = start_message ?? {};

                    switch (event.type) {
                        case "message_start":
                            start_message = event.message;
                            break
                        case "message_delta":
                            output_tokens = event.usage.output_tokens;
                            stop_reason = event.delta.stop_reason;
                            break
                        case "message_stop":
                            break
                        case "content_block_start":
                            yield {
                                generated: event.content_block.text,
                                model_name,
                                meta: {},
                            }
                            break
                        case "content_block_delta":
                            yield {
                                generated: event.delta.text,
                                model_name,
                                meta: {},
                            }
                            break
                        case "content_block_stop":
                            break
                    }
                }

                if (last_event !== null) {
                    const input_tokens = start_message?.usage.input_tokens ?? 0;
                    yield {
                        generated: "",
                        model_name: "",
                        meta: {
                            baml_is_complete: stop_reason !== null && stop_reason !== "max_tokens",
                            prompt_tokens: input_tokens,
                            output_tokens,
                            total_tokens: input_tokens + output_tokens,
                            finish_reason: stop_reason,
                            stream: true,
                        },
                    };
                }
            }
        };
    }
}

clientManager.registerProvider("baml-anthropic", {
    createClient: (name: string, options: LLMBaseProviderArgs): LLMBaseProvider => {
        return new AnthropicClient(options);
    },
});

clientManager.registerProvider("baml-anthropic-chat", {
    createClient: (name: string, options: LLMBaseProviderArgs): LLMBaseProvider => {
        return new AnthropicClient(options);
    },
});

