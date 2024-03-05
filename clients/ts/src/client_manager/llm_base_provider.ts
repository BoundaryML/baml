import { logLLMEvent } from "baml-client-lib";
import { BaseProvider } from "./base_provider";

interface LLMChatMessage {
  role: string;
  content: string;
};

interface LLMResponse {
  generated: string,
  model_name: string,
  meta?: {
    [key: string]: any
  }
}

type LLMBaseProviderArgs = {
  provider: string;
  retry_policy?: any,
  redactions?: string[],
} & { [key: string]: any };

type Prompt = string | LLMChatMessage[];

function redact(value: any): any {
  if (typeof value === 'string') {
    if (value.length > 4) {
      return value.substring(0, 2) + '****';
    }
    return '****';
  }

  if (Array.isArray(value)) {
    return value.map(redact);
  }

  if (typeof value === 'object') {
    return Object.fromEntries(Object.entries(value).map(([k, v]) => [k, redact(v)]));
  }

  return redact(`${value}`);
}

abstract class LLMBaseProvider extends BaseProvider {
  private provider: string;
  private retry_policy: any;
  private redactions: string[];
  private client_args: { [key: string]: any };

  constructor(args: LLMBaseProviderArgs) {
    const { provider, retry_policy, redactions = [], ...rest } = args;
    if (Object.keys(rest).length) {
      throw new Error(`Unknown arguments: ${Object.keys(rest).join(', ')}`);
    }

    if (typeof provider !== 'string') {
      throw new Error(`provider must be a string: ${provider}`);
    }

    super();

    this.provider = provider;
    this.retry_policy = retry_policy;
    this.redactions = redactions;
    this.client_args = {};
  }

  protected set_args(args: { [key: string]: any }) {
    this.client_args = Object.fromEntries(Object.entries(args).map(([k, v]) => [k, redact(v)]));
  }

  abstract run_prompt(prompt: string): Promise<LLMResponse>;
  abstract run_chat(prompt: LLMChatMessage | LLMChatMessage[]): Promise<LLMResponse>;

  abstract run_prompt_template(prompt: string, template_args: Iterable<string>, params: {
    [key: string]: any
  }): Promise<LLMResponse>;
  abstract run_chat_template(prompt: LLMChatMessage | LLMChatMessage[], template_args: Iterable<string>, params: {
    [key: string]: any
  }): Promise<LLMResponse>;


  protected start_run(prompt: Prompt) {
    logLLMEvent({
      name: 'llm_request_start',
      data: {
        provider: this.provider,
        prompt: prompt,
      }
    });

    logLLMEvent({
      name: 'llm_request_args',
      data: {
        invocation_params: Object.fromEntries(Object.entries(this.client_args).map(([k, v]) => [k, JSON.stringify(v)])),
      }
    })
  }

  protected end_run(response: LLMResponse) {
    logLLMEvent({
      name: 'llm_request_end',
      data: {
        generated: response.generated,
        model_name: response.model_name,
        metadata: response.meta,
      }
    });
  }
}

export { LLMBaseProvider, LLMChatMessage, LLMBaseProviderArgs, LLMResponse };
