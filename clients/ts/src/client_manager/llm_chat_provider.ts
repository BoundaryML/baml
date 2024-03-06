import { logLLMEvent } from "@boundaryml/baml-ffi";
import { LLMBaseProvider, LLMBaseProviderArgs, LLMChatMessage, LLMResponse } from "./llm_base_provider";
import format from 'string-format';

interface LLMChatProviderArgs extends LLMBaseProviderArgs {
  prompt_to_chat: (prompt: string) => LLMChatMessage;
}

abstract class LLMChatProvider extends LLMBaseProvider {
  private prompt_to_chat: (prompt: string) => LLMChatMessage;

  constructor(args: LLMChatProviderArgs) {
    const { prompt_to_chat, ...rest } = args;
    super(rest);
    this.prompt_to_chat = prompt_to_chat;
  }

  run_prompt(prompt: string): Promise<LLMResponse> {
    return this.run_chat([this.prompt_to_chat(prompt)]);
  }
  run_prompt_template(prompt: string, template_args: Array<string>, params: { [key: string]: any; }): Promise<LLMResponse> {
    return this.run_chat_template([this.prompt_to_chat(prompt)], template_args, params);
  }

  run_chat(prompt: LLMChatMessage | LLMChatMessage[]): Promise<LLMResponse> {
    const prompts = Array.isArray(prompt) ? prompt : [prompt];
    this.start_run(prompts);
    return this.chat_with_telemetry(prompts);
  }
  run_chat_template(prompt: LLMChatMessage | LLMChatMessage[], template_args: Array<string>, params: { [key: string]: any; }): Promise<LLMResponse> {
    const prompts = Array.isArray(prompt) ? prompt : [prompt];

    const updates = template_args.map((arg): [string, string] => [arg, format(arg, params)]
    );

    this.start_run(prompts);
    logLLMEvent({
      name: 'llm_prompt_template',
      data: {
        template: prompts,
        template_args: Object.fromEntries(updates),
      }
    });
    prompts.forEach((prompt) => {
      let content = prompt.content;
      updates.forEach(([arg, value]) => {
        content = content.replaceAll(arg, value);
      });
      prompt.content = content;
    });

    return this.chat_with_telemetry(prompts);
  }

  private async chat_with_telemetry(prompt: LLMChatMessage[]): Promise<LLMResponse> {
    try {
      const result = await this.chat_impl(prompt);
      this.end_run(result);
      return result;
    } catch (err) {
      this.raise_error(err);
    }
  }

  /// Method to be implemented by the derived class
  protected abstract chat_impl(prompt: LLMChatMessage[]): Promise<LLMResponse>;
}

export { LLMChatProvider, LLMChatProviderArgs };
