/**
 * Transport-agnostic port for communicating with the BAML WASM runtime.
 *
 * Implementations:
 *   - WorkerRuntimePort: wraps a Web Worker (promptfiddle — same worker
 *     that MonacoEditor already created for LSP + execution)
 *   - (future) VS Code webview port via postMessage bridge
 */

import type { WorkerOutMessage, WorkerInMessage } from './worker-protocol';

export interface RuntimePort {
  /** Send a command to the runtime. */
  postMessage(msg: WorkerInMessage): void;

  /** Subscribe to messages from the runtime. Returns an unsubscribe function. */
  onMessage(handler: (msg: WorkerOutMessage) => void): () => void;

  /** Clean up listeners. */
  dispose(): void;
}
