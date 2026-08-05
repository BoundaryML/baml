import { ToolingRequest, ToolingResponse } from './generated/tooling.js';

export interface ToolingBackend {
  readonly kind: 'native' | 'wasm';
  dispatch(request: Uint8Array): Uint8Array;
  dispose?(): void;
}

export function request(
  backend: ToolingBackend,
  value: ToolingRequest,
): ToolingResponse {
  const bytes = ToolingRequest.encode(value).finish();
  const response = ToolingResponse.decode(backend.dispatch(bytes));
  if (response.response?.$case === 'error') {
    const error = new Error(response.response.error.message);
    Object.assign(error, { code: response.response.error.code });
    throw error;
  }
  if (!response.response)
    throw new Error('BAML tooling bridge returned an empty response');
  return response;
}

export interface NativeAddon {
  BamlToolingBridge: new () => { dispatch(request: Uint8Array): Uint8Array };
}

export class NativeBackend implements ToolingBackend {
  readonly kind = 'native' as const;
  readonly #bridge: { dispatch(request: Uint8Array): Uint8Array };

  constructor(addon: NativeAddon) {
    this.#bridge = new addon.BamlToolingBridge();
  }

  dispatch(value: Uint8Array): Uint8Array {
    return this.#bridge.dispatch(value);
  }
}

export class WasmBackend implements ToolingBackend {
  readonly kind = 'wasm' as const;
  constructor(readonly toolingRequest: (request: Uint8Array) => Uint8Array) {}
  dispatch(value: Uint8Array): Uint8Array {
    return this.toolingRequest(value);
  }
}
