import { ChatCompletionCreateParams } from "openai/resources/chat/completions";
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
        this.client = new OpenAI({
            apiKey: api_key,
            organization: organization,
            baseURL: base_url,
            timeout: timeout,
            maxRetries: max_retries ?? 0,
        });

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
}

clientManager.registerProvider("baml-openai-chat", {
    createClient: (name: string, options: LLMBaseProviderArgs): LLMChatProvider => {
        return new OpenAIClient(options);
    },
});

clientManager.registerProvider("baml-azure-chat", {
    createClient: (name: string, options: LLMBaseProviderArgs): LLMChatProvider => {
        return new OpenAIClient(options);
    },
});
