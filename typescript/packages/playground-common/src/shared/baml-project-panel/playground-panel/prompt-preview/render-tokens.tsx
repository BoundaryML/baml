import {
  type Tiktoken,
  type TiktokenEncoding,
  type TiktokenModel,
  getEncoding,
  getEncodingNameForModel,
} from 'js-tiktoken';
// We need this cache because loading a new encoder for every snippet makes rendering horribly slow
export class TokenEncoderCache {
  static SUPPORTED_PROVIDERS = [
    'baml-openai-chat',
    'baml-openai-completion',
    'baml-azure-chat',
    'baml-azure-completion',
  ];
  static INSTANCE = new TokenEncoderCache();

  encoders: Map<TiktokenEncoding, Tiktoken>;

  private constructor() {
    this.encoders = new Map();
  }

  static getEncodingNameForModel(
    provider: string,
    model: string,
  ): TiktokenEncoding | undefined {
    if (!TokenEncoderCache.SUPPORTED_PROVIDERS.includes(provider))
      return undefined;

    // We have to use this try-catch approach because tiktoken does not expose a list of supported models
    try {
      return getEncodingNameForModel(model as TiktokenModel);
    } catch {
      return undefined;
    }
  }

  getEncoder(encoding: TiktokenEncoding): Tiktoken {
    const cached = this.encoders.get(encoding);
    if (cached) return cached;

    const encoder = getEncoding(encoding);
    this.encoders.set(encoding, encoder);
    return encoder;
  }
}
