// Actual TypeScript explicit-resource-management syntax compiled to ES2022.
// This is an ownership model, not a production BAML bridge test.

function assert(condition: unknown, message: string): asserts condition {
  if (!condition) throw new Error(message);
}

function assertEqual<T>(actual: T, expected: T, message: string): void {
  if (actual !== expected) throw new Error(`${message}: expected ${String(expected)}, got ${String(actual)}`);
}

function deferred<T>(): {
  readonly promise: Promise<T>;
  resolve(value: T): void;
  reject(error: unknown): void;
} {
  let resolve!: (value: T) => void;
  let reject!: (error: unknown) => void;
  const promise = new Promise<T>((resolvePromise, rejectPromise) => {
    resolve = resolvePromise;
    reject = rejectPromise;
  });
  return { promise, resolve, reject };
}

const syncEvents: string[] = [];

class SyncResource implements Disposable {
  #disposed = false;

  constructor(private readonly name: string) {}

  [Symbol.dispose](): void {
    if (this.#disposed) return;
    this.#disposed = true;
    syncEvents.push(`dispose:${this.name}`);
  }
}

{
  using normal = new SyncResource("normal");
  syncEvents.push("body:normal");
  void normal;
}
assertEqual(syncEvents.join(","), "body:normal,dispose:normal", "using normal-exit order");

try {
  using throwing = new SyncResource("throw");
  void throwing;
  syncEvents.push("body:throw");
  throw new Error("body failure");
} catch (error) {
  assert(error instanceof Error && error.message === "body failure", "throwing body error changed");
}
assertEqual(
  syncEvents.join(","),
  "body:normal,dispose:normal,body:throw,dispose:throw",
  "using did not dispose during throw unwinding",
);

const asyncEvents: string[] = [];
const allowAsyncCleanup = deferred<void>();

class AsyncResource implements AsyncDisposable {
  #disposePromise: Promise<void> | undefined;

  [Symbol.asyncDispose](): Promise<void> {
    if (!this.#disposePromise) {
      this.#disposePromise = (async () => {
        asyncEvents.push("cleanup:start");
        await allowAsyncCleanup.promise;
        asyncEvents.push("cleanup:end");
      })();
    }
    return this.#disposePromise;
  }
}

let asyncBlockFinished = false;
const asyncBlock = (async () => {
  await using resource = new AsyncResource();
  asyncEvents.push("body");
  void resource;
})().then(() => {
  asyncBlockFinished = true;
});

await Promise.resolve();
await Promise.resolve();
assertEqual(asyncEvents.join(","), "body,cleanup:start", "await using did not start cleanup");
assert(!asyncBlockFinished, "await using did not await async cleanup");
allowAsyncCleanup.resolve();
await asyncBlock;
assertEqual(asyncEvents.join(","), "body,cleanup:start,cleanup:end", "async cleanup did not finish");

interface GreeterHost {
  greet(name: string, gate?: Promise<void>): Promise<string>;
}

interface ReceiverState {
  readonly id: string;
  readonly scopeId: number;
  readonly host: GreeterHost;
  retained: boolean;
  revoked: boolean;
  refOwners: number;
  readonly activeCalls: Set<Promise<unknown>>;
}

// Suitable as detached/finalizer-held state: IDs and an idempotence bit only.
// It contains neither the proxy nor the user's host implementation.
interface DetachedLeaseState {
  readonly runtimeId: number;
  readonly receiverId: string;
  readonly leaseId: number;
  released: boolean;
}

let nextRuntimeId = 0;
const runtimeCores = new Map<number, RuntimeCore>();

class RuntimeCore {
  readonly runtimeId = ++nextRuntimeId;
  readonly #receivers = new Map<string, ReceiverState>();
  readonly #leases = new Map<number, string>();
  #nextReceiverId = 0;
  #nextLeaseId = 0;

  constructor() {
    runtimeCores.set(this.runtimeId, this);
  }

  createRetainedReceiver(scopeId: number, host: GreeterHost): string {
    const id = `runtime-${this.runtimeId}:receiver-${++this.#nextReceiverId}`;
    this.#receivers.set(id, {
      id,
      scopeId,
      host,
      retained: true,
      revoked: false,
      refOwners: 0,
      activeCalls: new Set(),
    });
    return id;
  }

  acquireProxy(receiverId: string): GreeterProxy {
    const receiver = this.getUsable(receiverId);
    const leaseId = ++this.#nextLeaseId;
    receiver.refOwners += 1;
    this.#leases.set(leaseId, receiverId);
    return GreeterProxy.create({ runtimeId: this.runtimeId, receiverId, leaseId, released: false });
  }

  cloneProxy(lease: DetachedLeaseState): GreeterProxy {
    this.checkLease(lease);
    return this.acquireProxy(lease.receiverId);
  }

  releaseProxy(lease: DetachedLeaseState): void {
    if (lease.released) return;
    lease.released = true;
    const receiverId = this.#leases.get(lease.leaseId);
    this.#leases.delete(lease.leaseId);
    if (!receiverId) return;
    const receiver = this.#receivers.get(receiverId);
    if (receiver) receiver.refOwners -= 1;
  }

  async invoke(lease: DetachedLeaseState, name: string, gate?: Promise<void>): Promise<string> {
    this.checkLease(lease);
    const receiver = this.getUsable(lease.receiverId);
    const operation = receiver.host.greet(name, gate);
    receiver.activeCalls.add(operation);
    try {
      return await operation;
    } finally {
      receiver.activeCalls.delete(operation);
    }
  }

  async revokeScope(scopeId: number): Promise<void> {
    const receivers = [...this.#receivers.values()].filter((receiver) => receiver.scopeId === scopeId);
    for (const receiver of receivers) {
      receiver.revoked = true;
      receiver.retained = false;
    }
    await Promise.allSettled(receivers.flatMap((receiver) => [...receiver.activeCalls]));
  }

  receiverAlive(receiverId: string): boolean {
    const receiver = this.#receivers.get(receiverId);
    return !!receiver && (receiver.retained || receiver.refOwners > 0 || receiver.activeCalls.size > 0);
  }

  activeCallCount(scopeId: number): number {
    return [...this.#receivers.values()]
      .filter((receiver) => receiver.scopeId === scopeId)
      .reduce((total, receiver) => total + receiver.activeCalls.size, 0);
  }

  private checkLease(lease: DetachedLeaseState): void {
    if (lease.runtimeId !== this.runtimeId || lease.released || !this.#leases.has(lease.leaseId)) {
      throw new Error("proxy lease is released");
    }
  }

  private getUsable(receiverId: string): ReceiverState {
    const receiver = this.#receivers.get(receiverId);
    if (!receiver || receiver.revoked) throw new Error("scope is revoked");
    return receiver;
  }
}

function coreFor(lease: DetachedLeaseState): RuntimeCore {
  const core = runtimeCores.get(lease.runtimeId);
  if (!core) throw new Error("runtime is gone");
  return core;
}

export class GreeterProxy implements Disposable {
  private constructor(private readonly lease: DetachedLeaseState) {}

  static create(lease: DetachedLeaseState): GreeterProxy {
    return new GreeterProxy(lease);
  }

  clone(): GreeterProxy {
    return coreFor(this.lease).cloneProxy(this.lease);
  }

  greet(name: string, gate?: Promise<void>): Promise<string> {
    return coreFor(this.lease).invoke(this.lease, name, gate);
  }

  [Symbol.dispose](): void {
    coreFor(this.lease).releaseProxy(this.lease);
  }
}

let nextScopeId = 0;

export class BridgeScope implements AsyncDisposable {
  readonly scopeId = ++nextScopeId;
  readonly #core = new RuntimeCore();
  #closePromise: Promise<void> | undefined;
  closeCompleted = false;

  retain(host: GreeterHost): { readonly receiverId: string; project(): GreeterProxy } {
    const receiverId = this.#core.createRetainedReceiver(this.scopeId, host);
    return {
      receiverId,
      project: () => this.#core.acquireProxy(receiverId),
    };
  }

  activeCallCount(): number {
    return this.#core.activeCallCount(this.scopeId);
  }

  receiverAlive(receiverId: string): boolean {
    return this.#core.receiverAlive(receiverId);
  }

  close(): Promise<void> {
    if (!this.#closePromise) {
      this.#closePromise = this.#core.revokeScope(this.scopeId).then(() => {
        this.closeCompleted = true;
      });
    }
    return this.#closePromise;
  }

  [Symbol.asyncDispose](): Promise<void> {
    return this.close();
  }
}

const host: GreeterHost = {
  async greet(name, gate) {
    if (gate) await gate;
    return `Hello, ${name}!`;
  },
};

// Disposing a proxy only releases that owner. The scope-retained receiver and
// another clone remain usable, and a fresh projection keeps the same identity.
const ownershipScope = new BridgeScope();
const retained = ownershipScope.retain(host);
const firstProxy = retained.project();
const clonedProxy = firstProxy.clone();
firstProxy[Symbol.dispose]();
firstProxy[Symbol.dispose]();
assert(ownershipScope.receiverAlive(retained.receiverId), "disposing one proxy killed retained receiver");
assert((await clonedProxy.greet("clone")) === "Hello, clone!", "clone did not survive peer disposal");
clonedProxy[Symbol.dispose]();
assert(ownershipScope.receiverAlive(retained.receiverId), "dropping proxies killed scope-retained receiver");
using reprojected = retained.project();
assert((await reprojected.greet("again")) === "Hello, again!", "retained receiver could not be reprojected");
await ownershipScope.close();
await ownershipScope.close();
assert(ownershipScope.closeCompleted, "scope close was not idempotent/completed");

// Async scope disposal revokes immediately, rejects new operations, and waits
// for an operation already holding a call lease to finish.
const pendingGate = deferred<void>();
const pendingScope = new BridgeScope();
const pendingRetained = pendingScope.retain(host);
const pendingProxy = pendingRetained.project();
const pendingOperation = pendingProxy.greet("pending", pendingGate.promise);
await Promise.resolve();
assertEqual(pendingScope.activeCallCount(), 1, "pending operation was not retained");
let closeFinished = false;
const closeOperation = pendingScope.close().then(() => {
  closeFinished = true;
});
await Promise.resolve();
assert(!closeFinished, "scope close did not await the pending operation");
await (async () => {
  try {
    await pendingProxy.greet("late");
    throw new Error("new call unexpectedly succeeded after revocation");
  } catch (error) {
    assert(error instanceof Error && error.message === "scope is revoked", "unexpected revocation error");
  }
})();
pendingGate.resolve();
assertEqual(await pendingOperation, "Hello, pending!", "retained pending operation did not complete");
await closeOperation;
assert(closeFinished, "scope close did not finish after call completion");
pendingProxy[Symbol.dispose]();
pendingProxy[Symbol.dispose]();

// Structured example: every operation is awaited in the lexical scope. Reverse
// disposal releases the proxy before await-using closes the scope.
let structuredScope: BridgeScope | undefined;
async function noUnawaitedOperationEscapes(): Promise<void> {
  await using scope = new BridgeScope();
  structuredScope = scope;
  const receiver = scope.retain(host);
  using proxy = receiver.project();
  assertEqual(await proxy.greet("structured"), "Hello, structured!", "structured call failed");
  assertEqual(scope.activeCallCount(), 0, "awaited operation remained active");
}
await noUnawaitedOperationEscapes();
assert(structuredScope?.closeCompleted, "await using returned before scope cleanup completed");

console.log("disposal probe passed");
console.log("using disposed on normal/throw exits; await using awaited cleanup");
console.log("proxy release, retained receiver lifetime, scope revocation/drain, and idempotence passed");
