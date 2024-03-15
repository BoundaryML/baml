import { clientManager } from "../client_manager";
import { LLMException, ProviderErrorCode } from "../errors";
import { LLMBaseProvider, LLMBaseProviderArgs, LLMChatMessage, LLMResponse } from "../llm_base_provider";

class FallbackClient extends LLMBaseProvider {
    private fallbackNames: string[];

    constructor(params: LLMBaseProviderArgs & {
        strategy?: {client: string}[]
    }) {
        const {
            strategy,
            ...rest
        } = params;

        if (!strategy) {
            throw new Error("No fallback strategy provided");
        }

        super(rest);
        this.fallbackNames = strategy.map((s) => s.client);
    }


    private fallbacks(): LLMBaseProvider[] {
        return this.fallbackNames.map((name) => clientManager.getClient(name));
    }

    private async try_fallback(fn: (fallback: LLMBaseProvider) => Promise<LLMResponse>): Promise<LLMResponse> {
        const fallbacks = this.fallbacks();
        for (let i = 0; i < fallbacks.length; i++) {
            const fallback = fallbacks[i];
            try {
                return await fn(fallback);
            } catch (e) {
                if (i === fallbacks.length - 1) {
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
        return new FallbackClient(options);
    },
});
