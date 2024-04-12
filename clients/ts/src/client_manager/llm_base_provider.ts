import { NapiClient } from "@boundaryml/baml-core-ffi";
import { FireBamlEvent } from "../ffi_layer";
import { BaseProvider } from "./base_provider";
import { RetryPolicy } from "./retry_policy"

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
  private readonly provider: string;
  private readonly retry_policy: RetryPolicy;
  private redactions: string[];
  private client_args: { [key: string]: any };
  protected readonly napi_client: NapiClient;

  constructor(args: LLMBaseProviderArgs) {
    const { provider, retry_policy, redactions = [], client_name = '<unknown>', ...rest } = args;
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
    this.napi_client = new NapiClient(
      client_name,
      provider
    );
  }

  protected set_args(args: { [key: string]: any }) {
    this.client_args = Object.fromEntries(Object.entries(args).map(([k, v]) => [k, redact(v)]));
  }

  async run_jinja_template(
    jinja_template: string,
    args: { [key: string]: any },
    output_schema: string,
    template_macros: any[]
  ): Promise<LLMResponse> {
    if (this.retry_policy) {
      return await this.retry_policy.run(() => this.run_jinja_template_once(jinja_template, args, output_schema, template_macros));
    }

    return await this.run_jinja_template_once(jinja_template, args, output_schema, template_macros);
  }
  protected abstract run_jinja_template_once(
    jinja_template: string,
    args: { [key: string]: any },
    output_schema: string,
    template_macros: any[]
  ): Promise<LLMResponse>;


  async run_prompt(prompt: string): Promise<LLMResponse> {
    if (this.retry_policy) {
      return await this.retry_policy.run(() => this.run_prompt_once(prompt));
    }
    return await this.run_prompt_once(prompt);
  }
  protected abstract run_prompt_once(prompt: string): Promise<LLMResponse>;

  async run_chat(prompt: LLMChatMessage | LLMChatMessage[]): Promise<LLMResponse> {
    if (this.retry_policy) {
      return await this.retry_policy.run(() => this.run_chat_once(prompt));
    }
    return await this.run_chat_once(prompt);
  }
  protected abstract run_chat_once(prompt: LLMChatMessage | LLMChatMessage[]): Promise<LLMResponse>;

  async run_prompt_template(prompt: string, template_args: Iterable<string>, params: {
    [key: string]: any
  }): Promise<LLMResponse> {
    if (this.retry_policy) {
      return await this.retry_policy.run(() => this.run_prompt_template_once(prompt, template_args, params));
    }
    return await this.run_prompt_template_once(prompt, template_args, params);
  }
  protected abstract run_prompt_template_once(prompt: string, template_args: Iterable<string>, params: {
    [key: string]: any
  }): Promise<LLMResponse>;

  async run_chat_template(prompt: LLMChatMessage | LLMChatMessage[], template_args: Iterable<string>, params: {
    [key: string]: any
  }): Promise<LLMResponse> {
    if (this.retry_policy) {
      return await this.retry_policy.run(() => this.run_chat_template_once(prompt, template_args, params));
    }
    return await this.run_chat_template_once(prompt, template_args, params);
  }
  protected abstract run_chat_template_once(prompt: LLMChatMessage | LLMChatMessage[], template_args: Iterable<string>, params: {
    [key: string]: any
  }): Promise<LLMResponse>;


  protected start_run(prompt: Prompt) {
    FireBamlEvent.llmStart({
      provider: this.provider,
      prompt: prompt,
    });

    FireBamlEvent.llmArgs(Object.fromEntries(
      Object.entries(this.client_args).map(([k, v]) => [k, JSON.stringify(v)])
    ))
  }

  protected end_run(response: LLMResponse) {
    FireBamlEvent.llmEnd({
      generated: response.generated,
      model_name: response.model_name,
      metadata: response.meta,
    });
  }
}

export { LLMBaseProvider, LLMChatMessage, LLMBaseProviderArgs, LLMResponse };
