/**
 * RuntimePort backed by a Web Worker.
 *
 * Used in promptfiddle where MonacoEditor spawns the single WASM worker
 * (handling both LSP and custom RPC). This class wraps that existing Worker
 * so the ExecutionPanel can communicate with it through the RuntimePort
 * interface.
 *
 * Uses addEventListener (not worker.onmessage =) so it never clobbers
 * any other listener on the worker.
 *
 * IMPORTANT: This does NOT create or own the worker. MonacoEditor owns the
 * worker lifecycle. dispose() only removes our listener.
 */

import type { RuntimePort } from '../runtime-port';
import type { WorkerOutMessage, WorkerInMessage } from '../worker-protocol';

export class WorkerRuntimePort implements RuntimePort {
  private _handlers = new Set<(msg: WorkerOutMessage) => void>();
  private _worker: Worker;
  private _listener: (event: MessageEvent) => void;

  constructor(worker: Worker) {
    this._worker = worker;

    this._listener = (event: MessageEvent) => {
      const data = event.data;
      if (!data || typeof data !== 'object' || !('type' in data)) return;

      for (const handler of this._handlers) {
        handler(data as WorkerOutMessage);
      }
    };

    worker.addEventListener('message', this._listener);
  }

  postMessage(msg: WorkerInMessage): void {
    // Transfer the argsProto buffer for callFunction to avoid copying
    if (msg.type === 'callFunction') {
      const buffer = msg.argsProto.buffer;
      this._worker.postMessage(msg, [buffer]);
    } else {
      this._worker.postMessage(msg);
    }
  }

  onMessage(handler: (msg: WorkerOutMessage) => void): () => void {
    this._handlers.add(handler);
    return () => {
      this._handlers.delete(handler);
    };
  }

  dispose(): void {
    this._worker.removeEventListener('message', this._listener);
    this._handlers.clear();
  }
}
