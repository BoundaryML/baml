import { logLLMEvent } from "baml-client-lib";

export type ChatMessage = {
    role: string;
    content: string;
};

export interface IClient {
    run_chat: (prompt: ChatMessage[] | ChatMessage) => Promise<string>;
    run_prompt: (prompt: string) => Promise<string>;
    run_chat_template: (prompt_template: ChatMessage[] | ChatMessage, templates: {
        [key: string]: string;
    }) => Promise<string>;
    run_prompt_template: (prompt_template: string, templates: {
        [key: string]: string;
    }) => Promise<string>;
}

class ClientManager {
    private clients: Map<string, IClient> = new Map();
    private providers: Map<string, IProvider> = new Map();


    getClient(name: string): IClient {
        const client = this.clients.get(name);
        if (!client) {
            throw new Error(`Client ${name} not found`);
        }
        return client;
    }

    createClient(name: string, provider: string, options: {
        [key: string]: any;
    }): IClient {
        const provider_ = this.providers.get(provider);
        if (!provider_) {
            throw new Error(`Provider ${provider} not found`);
        }
        const client = provider_.createClient(name, options);
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
    createClient: (name: string, options: {
        [key: string]: any;
    }) => IClient;
}

export const clientManager = new ClientManager();


// ----
class LLMException extends Error {
    code: number;
    constructor(message: string, code: number) {
        super(message);
        this.code = code;
    }
}

abstract class BaseProvider {
    private __to_error_code(err: Error): number | undefined {
        if (err instanceof LLMException) {
            return err.code;
        }
        const code = this._to_error_code(err);
        if (code !== undefined) {
            return code;
        }

        return undefined;
    }

    protected abstract _to_error_code(err: Error): number | undefined;

    protected _raise_error(err: Error): never {
        const formatted_traceback = err.stack?.split("\n").map((line) => `    ${line}`).join("\n");
        logLLMEvent({
            name: 'llm_request_error',
            data: {
                error_code: this.__to_error_code(err) ?? 1,
                message: err.message,
                traceback: formatted_traceback,
            }
        });
        if (err instanceof LLMException) {
            throw err;
        }
        const code = this.__to_error_code(err);
        if (code !== undefined) {
            throw new LLMException(err.message, code);
        }
        throw err;
    }
}

function update_template_args(template: string, args: {
    [key: string]: string;
}): string {
    Object.entries(args).forEach(([key, value]) => {
        template = template.replace(`{{${key}}}`, value);
    });

    return template;
}