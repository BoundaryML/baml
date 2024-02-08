import { CompletionCreateParamsNonStreaming } from "openai/resources/completions";
import { ChatMessage, clientManager, IClient } from "../client_manager";
import { OpenAI } from "openai";

class OpenAIClient implements IClient {
    private client: OpenAI;
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
        this.client = new OpenAI(constuctorParams);

        this.params = {
            model: paramMap.get("model"),
            frequency_penalty: paramMap.get("frequency_penalty") ?? paramMap.get("frequencyPenalty"),
            logit_bias: paramMap.get("logit_bias") ?? paramMap.get("logitBias"),
            logprobs: paramMap.get("logprobs"),
            max_tokens: paramMap.get("max_tokens") ?? paramMap.get("maxTokens"),
            n: 1,
            presence_penalty: paramMap.get("presence_penalty") ?? paramMap.get("presencePenalty"),
            seed: paramMap.get("seed"),
            stop: paramMap.get("stop"),
            stream: false,
            temperature: paramMap.get("temperature") ?? 0,
            top_p: paramMap.get("top_p") ?? paramMap.get("topP"),
            user: paramMap.get("user"),
        }
    }

    async run_chat(prompt: ChatMessage | ChatMessage[]): Promise<string> {
        const chats = Array.isArray(prompt) ? prompt : [prompt];
        const prompt_ = chats.map((chat) => `${chat.role}: ${chat.content}`).join("\n");
        return await this.run_prompt(prompt_);
    }
    async run_prompt(prompt: string): Promise<string> {
        const response = await this.client.completions.create({
            prompt,
            ...this.params,
        });

        const message = response.choices[0].text;
        if (message === undefined || message === null) {
            throw new Error("Message is undefined");
        }

        return message;
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
        Object.entries(templates).forEach(([key, value]) => {
            prompt_template = prompt_template.replaceAll(key, value);
        });

        return this.run_prompt(prompt_template);
    }
}

clientManager.registerProvider("baml-openai-completion", {
    createClient: (name: string, options: { [key: string]: any }): IClient => {
        return new OpenAIClient(options);
    },
});

clientManager.registerProvider("baml-azure-completion", {
    createClient: (name: string, options: { [key: string]: any }): IClient => {
        return new OpenAIClient(options);
    },
});
