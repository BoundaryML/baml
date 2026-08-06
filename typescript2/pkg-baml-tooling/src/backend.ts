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
  void backend;
  void value;
  throw new Error('not implemented');
}

export interface NativeAddon {
  BamlToolingBridge: new () => { dispatch(request: Uint8Array): Uint8Array };
}

/// One WASM tooling bridge instance. Construct a fresh bridge per backend —
/// each instance owns its protocol sessions, matching the native backend's
/// per-instance isolation; sharing one module-level dispatcher across
/// backends would let same-root sessions clobber each other.
export interface WasmBridge {
  dispatch(request: Uint8Array): Uint8Array;
}

export class NativeBackend implements ToolingBackend {
  readonly kind = 'native' as const;

  constructor(addon: NativeAddon) {
    void addon;
    throw new Error('not implemented');
  }

  dispatch(value: Uint8Array): Uint8Array {
    void value;
    throw new Error('not implemented');
  }
}

export class WasmBackend implements ToolingBackend {
  readonly kind = 'wasm' as const;
  constructor(readonly bridge: WasmBridge) {
    throw new Error('not implemented');
  }
  dispatch(value: Uint8Array): Uint8Array {
    void value;
    throw new Error('not implemented');
  }
}
