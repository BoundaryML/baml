import Anthropic from '@anthropic-ai/sdk';
import { ChatMessage, clientManager, IClient } from "../client_manager";
import { CompletionCreateParamsNonStreaming } from '@anthropic-ai/sdk/resources';

class AnthropicClient implements IClient {
    private client: Anthropic;
    private params: Omit<CompletionCreateParamsNonStreaming, 'prompt'>;

    constructor(params: { [key: string]: any }) {
        const paramMap = new Map(Object.entries(params));

        const constuctorParams = {
            apiKey: paramMap.get("apiKey") ?? paramMap.get("api_key"),
            organization: paramMap.get("organization"),
            baseURL: paramMap.get("baseURL") ?? paramMap.get("base_url"),
            timeout: paramMap.get("timeout"),
            maxRetries: paramMap.get("maxRetries") ?? paramMap.get("max_retries") ?? 0,
        };
        this.client = new Anthropic(constuctorParams);

        this.params = {
            model: paramMap.get("model"),
            max_tokens_to_sample: paramMap.get("max_tokens_to_sample") ?? paramMap.get("maxTokensToSample"),
            stop_sequences: paramMap.get("stop_sequences") ?? paramMap.get("stopSequences"),
            stream: false,
            temperature: paramMap.get("temperature") ?? 0,
            top_k: paramMap.get("top_k") ?? paramMap.get("topK"),
            top_p: paramMap.get("top_p") ?? paramMap.get("topP"),
            metadata: paramMap.get("metadata"),
        }
    }

    async run_chat(prompt: ChatMessage | ChatMessage[]): Promise<string> {
        const chats = Array.isArray(prompt) ? prompt : [prompt];
        const response = await this.client.completions.create({
            prompt: chats.map((chat) => ({
                role: chat.role === "user" ? Anthropic.HUMAN_PROMPT : Anthropic.AI_PROMPT,
                content: chat.content,
            })).join(""),
            ...this.params,
        });

        const message = response.completion
        if (message === undefined || message === null) {
            throw new Error("Message is undefined");
        }

        return message;
    }
    async run_prompt(prompt: string): Promise<string> {
        return await this.run_chat({
            role: "user",
            content: prompt,
        });
    }

    async run_chat_template(prompt_template: ChatMessage | ChatMessage[], templates: { [key: string]: string; }): Promise<string> {
        const chats = Array.isArray(prompt_template) ? prompt_template : [prompt_template];

        chats.forEach((chat) => {
            Object.entries(templates).forEach(([key, value]) => {
                chat.content = chat.content.replaceAll(key, value);
            });
        });

        return await this.run_chat(chats);
    }
    async run_prompt_template(prompt_template: string, templates: { [key: string]: string; }): Promise<string> {
        return this.run_chat_template({
            role: "user",
            content: prompt_template,
        }, templates);
    }
}

clientManager.registerProvider("baml-anthropic", {
    createClient: (name: string, options: { [key: string]: any }): IClient => {
        return new AnthropicClient(options);
    },
});
