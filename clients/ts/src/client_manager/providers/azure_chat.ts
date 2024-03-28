import { clientManager } from "../client_manager";
import { OpenAIClient, AzureKeyCredential, GetChatCompletionsOptions } from "@azure/openai";
import { LLMChatProvider } from "../llm_chat_provider";
import { LLMChatMessage, LLMResponse, LLMBaseProviderArgs } from "../llm_base_provider";

class AzureOpenAIClient extends LLMChatProvider {
    private client: OpenAIClient;
    private params: GetChatCompletionsOptions;
    private deployment: string;

    constructor(params: LLMBaseProviderArgs) {
        const {
            api_key,
            api_type,
            api_version,
            api_base,
            base_url,
            // Same
            endpoint,
            azure_endpoint,

            // Same
            deployment_name,
            engine,
            model,

            organization,
            timeout,
            max_retries,
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

        const passedEndpoint = endpoint ?? azure_endpoint;

        this.client = new OpenAIClient(passedEndpoint ?? base_url, {
            key: api_key,
        }, {
            apiVersion: api_version,
        });

        this.deployment = deployment_name ?? model ?? engine ?? "<unknown>";
        this.params = {
            maxTokens: max_tokens,
            presencePenalty: presence_penalty,
            frequencyPenalty: frequency_penalty,
            logitBias: logit_bias,
            stop,
            temperature,
            topP: top_p,
            responseFormat: response_format,
            user,
            seed,
        }
    }

    protected to_error_code_impl(err: unknown): number | undefined {
        return undefined;
    }

    protected async chat_impl(prompt: LLMChatMessage[]): Promise<LLMResponse> {
        const response = await this.client.getChatCompletions(this.deployment, prompt.map((chat) => ({
            role: chat.role as 'user',
            content: chat.content,
        })),
            {
                ...this.params,
                n: 1,
            });

        const choice = response.choices[0];
        if (choice === undefined || choice === null) {
            throw new Error("Choice is undefined");
        }

        const message = choice.message?.content;
        if (message === undefined || message === null) {
            throw new Error("Message is undefined");
        }

        return {
            generated: message,
            model_name: this.deployment,
            meta: {
                finish_reason: choice.finishReason,
                prompt_tokens: response.usage?.promptTokens,
                output_tokens: response.usage?.completionTokens,
                total_tokens: response.usage?.totalTokens,
            }
        }
    }
}

clientManager.registerProvider("baml-azure-chat", {
    createClient: (name: string, options: LLMBaseProviderArgs): LLMChatProvider => {
        return new AzureOpenAIClient(options);
    },
});
