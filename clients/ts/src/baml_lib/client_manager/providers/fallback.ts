import { ChatMessage, clientManager, IClient } from "../client_manager";

class FallbackClient implements IClient {
    private fallbackNames: string[];
    private fallbacks: IClient[];

    constructor(params: { [key: string]: any }) {
        this.fallbacks = [];
        this.fallbackNames = params.strategy;
    }

    async run_chat(prompt: ChatMessage | ChatMessage[]): Promise<string> {
        for (const fallback of this.fallbackNames) {
            try {
                const client = clientManager.getClient(fallback);
                return await client.run_chat(prompt);
            } catch (e) {
                console.log(e);
            }
        }
        throw new Error("All fallbacks failed");
    }
    async run_prompt(prompt: string): Promise<string> {
        for (const fallback of this.fallbackNames) {
            try {
                const client = clientManager.getClient(fallback);
                return await client.run_prompt(prompt);
            } catch (e) {
                console.log(e);
            }
        }
        throw new Error("All fallbacks failed");
    }

    async run_chat_template(prompt_template: ChatMessage | ChatMessage[], templates: { [key: string]: string; }): Promise<string> {
        for (const fallback of this.fallbackNames) {
            try {
                const client = clientManager.getClient(fallback);
                return await client.run_chat_template(prompt_template, templates);
            } catch (e) {
                console.log(e);
            }
        }
        throw new Error("All fallbacks failed");
    }
    async run_prompt_template(prompt_template: string, templates: { [key: string]: string; }): Promise<string> {
        for (const fallback of this.fallbackNames) {
            try {
                const client = clientManager.getClient(fallback);
                return await client.run_prompt_template(prompt_template, templates);
            } catch (e) {
                console.log(e);
            }
        }
        throw new Error("All fallbacks failed");
    }
}

clientManager.registerProvider("baml-fallback", {
    createClient: (name: string, options: { [key: string]: any }): IClient => {
        return new FallbackClient(options);
    },
});
