import { Ollama, ChatRequest } from 'ollama'
import { clientManager } from "../client_manager";
import { LLMChatProvider } from "../llm_chat_provider";
import { LLMChatMessage, LLMResponse, LLMBaseProviderArgs } from "../llm_base_provider";

class OllamaChatAIClient extends LLMChatProvider {
  private client: Ollama;
  private params: Omit<ChatRequest, 'stream'>;

  constructor(params: LLMBaseProviderArgs) {
    const {
      host = "http://127.0.0.1:11434",
      options,
      format,
      model,
      ...rest
    } = params;

    super({
      prompt_to_chat: (prompt) => {
        return {
          role: 'system',
          content: prompt,
        }
      },
      ...rest
    });

    if (host === undefined) {
      throw new Error("Missing host: consider adding 'host http://127.0.0.1:11434'");
    }

    if (model === undefined) {
      throw new Error("Missing model: consider adding 'model mistral'");
    }

    console.log(`Connecting to Ollama at ${host} with model ${model}`);
    this.client = new Ollama({
      host: host,
    });
    this.params = {
      model: model,
      format: format,
      options: options,
    }
  }

  protected to_error_code_impl(err: unknown): number | undefined {
    if (err instanceof Error) {
      if (err.message.includes("model not found")) {
        return 404;
      }
    }
    return undefined;
  }

  protected to_ollama_role(role: string): 'user' | 'system' | 'assistant' {
    switch (role) {
      case 'user':
      case 'system':
      case 'assistant':
        return role;
      default:
        return 'system';
    }
  }

  protected async chat_impl(prompt: LLMChatMessage[]): Promise<LLMResponse> {
    try {
      const response = await this.client.chat({
        messages: prompt.map((chat) => ({
          role: this.to_ollama_role(chat.role),
          content: chat.content,
        })),
        ...this.params,
        stream: false
      })

      return {
        generated: response.message.content,
        model_name: response.model,
        meta: {
          finish_reason: response.done ? "stop" : "interrupted",
          prompt_tokens: (response.prompt_eval_count ?? 0), // llama2 doesnt actually return this key
          output_tokens: response.eval_count ?? 0,
          total_tokens: (response.prompt_eval_count ?? 0) + (response.eval_count ?? 0),
        }
      }
    } catch (err) {
      console.log(JSON.stringify(err, null, 2));
      throw err;
    }
  }
}

clientManager.registerProvider("baml-ollama-chat", {
  createClient: (name: string, options: LLMBaseProviderArgs): LLMChatProvider => {
    return new OllamaChatAIClient(options);
  },
});
