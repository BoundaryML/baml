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
  private _nextFunctionCallRequestId = 1;
  private _pendingNextFunctionCalls = new Map<
    number,
    { resolve: (callId: number) => void; reject: (error: Error) => void }
  >();

  constructor(worker: Worker) {
    this._worker = worker;

    this._listener = (event: MessageEvent) => {
      const data = event.data;
      if (!data || typeof data !== 'object' || !('type' in data)) return;

      const msg = data as WorkerOutMessage;
      if (msg.type === 'nextFunctionCallResult') {
        const pending = this._pendingNextFunctionCalls.get(msg.id);
        if (pending) {
          this._pendingNextFunctionCalls.delete(msg.id);
          pending.resolve(msg.callId);
        }
        return;
      }
      if (msg.type === 'nextFunctionCallError') {
        const pending = this._pendingNextFunctionCalls.get(msg.id);
        if (pending) {
          this._pendingNextFunctionCalls.delete(msg.id);
          pending.reject(new Error(msg.error));
        }
        return;
      }

      for (const handler of this._handlers) {
        handler(msg);
      }
    };

    worker.addEventListener('message', this._listener);
  }

  nextFunctionCall(): Promise<number> {
    const id = this._nextFunctionCallRequestId++;
    return new Promise((resolve, reject) => {
      this._pendingNextFunctionCalls.set(id, { resolve, reject });
      this._worker.postMessage({ type: 'nextFunctionCall', id });
    });
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
    for (const pending of this._pendingNextFunctionCalls.values()) {
      pending.reject(new Error('Runtime port disposed'));
    }
    this._pendingNextFunctionCalls.clear();
    this._handlers.clear();
  }
}
