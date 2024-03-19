import { LLMBaseProvider, LLMBaseProviderArgs } from "./llm_base_provider";

export type ChatMessage = {
    role: string;
    content: string;
};


class ClientManager {
    private clients: Map<string, LLMBaseProvider> = new Map();
    private providers: Map<string, IProvider> = new Map();


    getClient(name: string): LLMBaseProvider {
        const client = this.clients.get(name);
        if (!client) {
            throw new Error(`Client ${JSON.stringify(name)} not found`);
        }
        return client;
    }

    createClient(name: string, provider: string, options: LLMBaseProviderArgs): LLMBaseProvider {
        const provider_ = this.providers.get(provider);
        if (!provider_) {
            throw new Error(`Provider ${provider} not found`);
        }
        const client = provider_.createClient(name, { ...options, provider });
        this.clients.set(name, client);
        return client;
    }

    registerProvider(name: string, provider: IProvider): void {
        if (this.providers.has(name)) {
            throw new Error(`Provider ${name} already registered`);
        }
        this.providers.set(name, provider);
    }
}
interface IProvider {
    createClient: (name: string, options: LLMBaseProviderArgs) => LLMBaseProvider;
}

export const clientManager = new ClientManager();
