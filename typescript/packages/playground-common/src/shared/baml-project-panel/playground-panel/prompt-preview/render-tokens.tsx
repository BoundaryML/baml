import { Tiktoken } from 'js-tiktoken/lite';

// Supported encoding types
type TiktokenEncoding = 'o200k_base' | 'cl100k_base';

// Lazy-loaded rank data - only imported when needed
const rankLoaders: Record<TiktokenEncoding, () => Promise<any>> = {
  o200k_base: () => import('js-tiktoken/ranks/o200k_base').then(m => m.default),
  cl100k_base: () => import('js-tiktoken/ranks/cl100k_base').then(m => m.default),
};

// Model to encoding mapping (simplified for common models)
const MODEL_TO_ENCODING: Record<string, TiktokenEncoding> = {
  // o200k_base models (GPT-4o family)
  'gpt-4o': 'o200k_base',
  'gpt-4o-mini': 'o200k_base',
  'gpt-4o-2024-05-13': 'o200k_base',
  'gpt-4o-2024-08-06': 'o200k_base',
  'gpt-4o-mini-2024-07-18': 'o200k_base',
  'o1': 'o200k_base',
  'o1-mini': 'o200k_base',
  'o1-preview': 'o200k_base',
  // cl100k_base models (GPT-4, GPT-3.5 family)
  'gpt-4': 'cl100k_base',
  'gpt-4-turbo': 'cl100k_base',
  'gpt-4-turbo-preview': 'cl100k_base',
  'gpt-4-0125-preview': 'cl100k_base',
  'gpt-4-1106-preview': 'cl100k_base',
  'gpt-4-32k': 'cl100k_base',
  'gpt-3.5-turbo': 'cl100k_base',
  'gpt-3.5-turbo-16k': 'cl100k_base',
  'gpt-35-turbo': 'cl100k_base', // Azure naming
  'gpt-35-turbo-16k': 'cl100k_base', // Azure naming
  'text-embedding-ada-002': 'cl100k_base',
  'text-embedding-3-small': 'cl100k_base',
  'text-embedding-3-large': 'cl100k_base',
};

// We need this cache because loading a new encoder for every snippet makes rendering horribly slow
export class TokenEncoderCache {
  static SUPPORTED_PROVIDERS = [
    'baml-openai-chat',
    'baml-openai-completion',
    'baml-azure-chat',
    'baml-azure-completion',
  ];
  static INSTANCE = new TokenEncoderCache();

  private encoders: Map<TiktokenEncoding, Tiktoken> = new Map();
  private loadingPromises: Map<TiktokenEncoding, Promise<Tiktoken>> = new Map();

  private constructor() {}

  static getEncodingNameForModel(
    provider: string,
    model: string,
  ): TiktokenEncoding | undefined {
    if (!TokenEncoderCache.SUPPORTED_PROVIDERS.includes(provider))
      return undefined;

    // Try exact match first
    if (MODEL_TO_ENCODING[model]) {
      return MODEL_TO_ENCODING[model];
    }

    // Try prefix matching for versioned model names
    const modelLower = model.toLowerCase();
    if (modelLower.startsWith('gpt-4o') || modelLower.startsWith('o1')) {
      return 'o200k_base';
    }
    if (modelLower.startsWith('gpt-4') || modelLower.startsWith('gpt-3.5') || modelLower.startsWith('gpt-35')) {
      return 'cl100k_base';
    }

    // Default to cl100k_base for unknown OpenAI models
    return 'cl100k_base';
  }

  async getEncoderAsync(encoding: TiktokenEncoding): Promise<Tiktoken> {
    // Return cached encoder if available
    const cached = this.encoders.get(encoding);
    if (cached) return cached;

    // Return existing loading promise if in progress
    const existing = this.loadingPromises.get(encoding);
    if (existing) return existing;

    // Start loading
    const loadPromise = (async () => {
      const ranks = await rankLoaders[encoding]();
      const encoder = new Tiktoken(ranks);
      this.encoders.set(encoding, encoder);
      this.loadingPromises.delete(encoding);
      return encoder;
    })();

    this.loadingPromises.set(encoding, loadPromise);
    return loadPromise;
  }

  // Synchronous getter - returns cached encoder or undefined
  getEncoder(encoding: TiktokenEncoding): Tiktoken | undefined {
    return this.encoders.get(encoding);
  }

  // Preload an encoding (fire and forget)
  preload(encoding: TiktokenEncoding): void {
    if (!this.encoders.has(encoding) && !this.loadingPromises.has(encoding)) {
      this.getEncoderAsync(encoding);
    }
  }
}
