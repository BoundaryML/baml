import Anthropic, { APIError, AnthropicError } from '@anthropic-ai/sdk';
import { clientManager } from "../client_manager";
import { MessageCreateParamsNonStreaming } from '@anthropic-ai/sdk/resources/beta/messages';
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
        const response = await this.client.beta.messages.create({
            messages: prompt.map((chat) => ({
                role: chat.role === "user" ? "user" : "assistant",
                content: chat.content,
            })),
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
}

clientManager.registerProvider("baml-anthropic", {
    createClient: (name: string, options: LLMBaseProviderArgs): LLMBaseProvider => {
        return new AnthropicClient(options);
    },
});
