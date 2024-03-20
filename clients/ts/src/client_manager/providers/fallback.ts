import { clientManager } from "../client_manager";
import { LLMException, ProviderErrorCode } from "../errors";
import { LLMBaseProvider, LLMBaseProviderArgs, LLMChatMessage, LLMResponse } from "../llm_base_provider";

// NOTE(sam): on_status_code is commented out b/c I don't think we should support it
interface FallbackStrategyEntry {
    client: string;
    // on_status_code?: number[];
}

class FallbackClient extends LLMBaseProvider {
    private fallbackClients: FallbackStrategyEntry[];

    constructor(readonly name: string, params: LLMBaseProviderArgs & {
        strategy?: (string | FallbackStrategyEntry)[]
    }) {
        const {
            strategy,
            ...rest
        } = params;

        if (!strategy) {
            throw new Error("No fallback strategy provided");
        }

        super(rest);
        if (!strategy) {
            throw new Error(`client<llm> ${this.name}: expected a strategy consisting of a list of clients, but none provided`);
        }

        if (strategy.length === 0) {
            throw new Error(`client<llm> ${this.name}: expected strategy to contain at least one client, but contained zero`);
        }

        this.fallbackClients = strategy.map((s) => typeof s === "string" ? { client: s } : s);
    }

    private async try_fallback(fn: (fallback: LLMBaseProvider) => Promise<LLMResponse>): Promise<LLMResponse> {
        for (let i = 0; i < this.fallbackClients.length; i++) {
            const fallback = clientManager.getClient(this.fallbackClients[i].client);
            try {
                return await fn(fallback);
            } catch (e) {
                if (i === this.fallbackClients.length - 1) {
                    throw e;
                }
            }
        }

        throw new LLMException("All fallbacks failed", ProviderErrorCode.NotFound);
    }


    async run_prompt_once(prompt: string): Promise<LLMResponse> {
        return this.try_fallback((fallback) => fallback.run_prompt(prompt));
    }

    async run_chat_once(prompt: LLMChatMessage | LLMChatMessage[]): Promise<LLMResponse> {
        return this.try_fallback((fallback) => fallback.run_chat(prompt));
    }

    async run_prompt_template_once(prompt: string, template_args: Iterable<string>, params: { [key: string]: any; }): Promise<LLMResponse> {
        return this.try_fallback((fallback) => fallback.run_prompt_template(prompt, template_args, params));
    }
    async run_chat_template_once(prompt: LLMChatMessage | LLMChatMessage[], template_args: Iterable<string>, params: { [key: string]: any; }): Promise<LLMResponse> {
        return this.try_fallback((fallback) => fallback.run_chat_template(prompt, template_args, params));
    }
    protected to_error_code_impl(err: unknown): number | undefined {
        return undefined
    }
}

clientManager.registerProvider("baml-fallback", {
    createClient: (name: string, options: LLMBaseProviderArgs): LLMBaseProvider => {
        return new FallbackClient(name, options);
    },
});
