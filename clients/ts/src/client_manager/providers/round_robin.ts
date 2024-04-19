import { clientManager } from "../client_manager";
import { LLMException, ProviderErrorCode } from "../errors";
import { LLMBaseProvider, LLMBaseProviderArgs, LLMChatMessage, LLMResponse } from "../llm_base_provider";

const ROUND_ROBIN_CLIENT_PROVIDER_ID = "baml-round-robin";

interface RoundRobinStrategyItem {
    client: string;
}

class RoundRobinClient extends LLMBaseProvider {
    private clients: RoundRobinStrategyItem[];
    private clientIndex: number;

    constructor(readonly name: string, params: LLMBaseProviderArgs & {
        strategy?: (string | RoundRobinStrategyItem)[],
        start?: number | 'random',
    }) {
        const {
            strategy,
            start = 'random',
            ...rest
        } = params;

        super(rest);

        if (!strategy) {
            throw new Error(`client<llm> ${this.name}: expected a strategy consisting of a list of clients, but none provided`);
        }

        if (strategy.length === 0) {
            throw new Error(`client<llm> ${this.name}: expected strategy to contain at least one client, but contained zero`);
        }

        this.clients = strategy.map((s) => typeof s === 'string' ? { client: s } : s);
        // Start from a random provider, so that if multiple processes are started at the same time,
        // they don't all start from the same provider.
        if (start === 'random') {
            this.clientIndex = Math.floor(Math.random() * this.clients.length);
        } else {
            if (!Number.isInteger(start)) {
                throw new Error(`client<llm> ${this.name}: expected start to be an int but is ${start}`);
            }
            if (!(0 <= start && start < this.clients.length)) {
                throw new Error(`client<llm> ${this.name}: expected start to specify an index in the list of clients but is ${start}`);
            }
            this.clientIndex = start;
        }
    }

    private choose_provider(): LLMBaseProvider {
        if (this.clients.length === 0) {
            throw new Error(`client<llm> ${this.name}: expected strategy to contain at least one client, but contained zero`);
        }
        // NB: in other languages, this should be an atomic get-and-increment operation, but in the
        // context of (1) the use case and (2) that Node is single-threaded (i.e. function calls are
        // never interleaved aka function calls are atomic), this is fine.
        const clientIndex = this.clientIndex;
        this.clientIndex = (this.clientIndex + 1) % this.clients.length;
        return clientManager.getClient(this.clients[clientIndex].client);
    }

    async run_jinja_template_once(jinja_template: string, args: { [key: string]: any; }, output_format: string, template_macros: {
        name: string;
        argNames: string[];
        argTypes: string[];
        template: string;
    }[]): Promise<LLMResponse> {
        return this.choose_provider().run_jinja_template(jinja_template, args, output_format, template_macros);
    }

    async run_prompt_once(prompt: string): Promise<LLMResponse> {
        return this.choose_provider().run_prompt(prompt);
    }

    async run_chat_once(prompt: LLMChatMessage | LLMChatMessage[]): Promise<LLMResponse> {
        return this.choose_provider().run_chat(prompt);
    }

    async run_prompt_template_once(prompt: string, template_args: Iterable<string>, params: { [key: string]: any; }): Promise<LLMResponse> {
        return this.choose_provider().run_prompt_template(prompt, template_args, params);
    }
    async run_chat_template_once(prompt: LLMChatMessage | LLMChatMessage[], template_args: Iterable<string>, params: { [key: string]: any; }): Promise<LLMResponse> {
        return this.choose_provider().run_chat_template(prompt, template_args, params);
    }
    protected to_error_code_impl(err: unknown): number | undefined {
        return undefined
    }
}

clientManager.registerProvider(ROUND_ROBIN_CLIENT_PROVIDER_ID, {
    createClient: (name: string, options: LLMBaseProviderArgs): LLMBaseProvider => {
        return new RoundRobinClient(name, options);
    },
});
