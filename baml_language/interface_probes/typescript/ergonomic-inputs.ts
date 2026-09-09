// Executable local model of a synchronous generated implementation factory and
// lazy runtime preparation. This does not exercise the production BAML bridge.

declare const greeterInputBrand: unique symbol;

export interface HostCallContext {
  readonly signal: AbortSignal;
}

export interface GreeterHost {
  greet: (name: string, ctx: HostCallContext) => string | Promise<string>;
}

export interface GreeterInput {
  readonly [greeterInputBrand]: true;
}

export interface GreeterImplementation extends GreeterInput {
  greet(name: string): Promise<string>;
  label(): Promise<string>;
}

interface Binding {
  readonly id: string;
  readonly host: GreeterHost;
  implementationRoot: boolean;
  refOwners: number;
  callLeases: number;
}

interface ImplementationState {
  readonly host: GreeterHost;
}

interface RefState {
  readonly runtime: MockRuntime;
  readonly binding: Binding;
  closed: boolean;
}

const implementationStates = new WeakMap<object, ImplementationState>();
const refStates = new WeakMap<object, RefState>();

function bindingIsActive(binding: Binding): boolean {
  return binding.implementationRoot || binding.refOwners > 0 || binding.callLeases > 0;
}

class GreeterImplementationValue implements GreeterImplementation {
  declare readonly [greeterInputBrand]: true;

  constructor(host: GreeterHost) {
    implementationStates.set(this, { host });
  }

  greet(name: string): Promise<string> {
    return invokeGreeterInput(this, "greet", [name], defaultRuntime);
  }

  label(): Promise<string> {
    return invokeGreeterInput(this, "label", [], defaultRuntime);
  }
}

export const Greeter = {
  implement(host: GreeterHost): GreeterImplementation {
    // Explicit implementation intent is recorded synchronously. There is no
    // runtime lookup or bind until an async generated call prepares this value.
    return new GreeterImplementationValue(host);
  },
};

export class GreeterRef implements GreeterInput {
  declare readonly [greeterInputBrand]: true;

  private constructor() {}

  static createForRuntime(runtime: MockRuntime, binding: Binding): GreeterRef {
    const ref = new GreeterRef();
    binding.refOwners += 1;
    refStates.set(ref, { runtime, binding, closed: false });
    return ref;
  }

  clone(): GreeterRef {
    const state = refStates.get(this)!;
    if (state.closed || !bindingIsActive(state.binding)) throw new Error("GreeterRef is closed");
    return GreeterRef.createForRuntime(state.runtime, state.binding);
  }

  close(): void {
    const state = refStates.get(this)!;
    if (state.closed) return;
    state.closed = true;
    state.binding.refOwners -= 1;
  }

  greet(name: string): Promise<string> {
    const state = refStates.get(this)!;
    return invokeGreeterInput(this, "greet", [name], state.runtime);
  }

  label(): Promise<string> {
    const state = refStates.get(this)!;
    return invokeGreeterInput(this, "label", [], state.runtime);
  }

  identityForProbe(): string {
    return refStates.get(this)!.binding.id;
  }
}

interface PreparedGreeter {
  readonly binding: Binding;
  release(): void;
}

let nextRuntimeId = 0;

export class MockRuntime {
  readonly #runtimeId = ++nextRuntimeId;
  readonly #implementationBindings = new WeakMap<object, Binding>();
  #nextBindingId = 0;
  hostBindCount = 0;
  defaultDispatchCount = 0;
  readonly invocationIdentities: string[] = [];

  async prepare(input: GreeterInput): Promise<PreparedGreeter> {
    // Checked refs take precedence. A closed/cross-runtime ref must fail rather
    // than falling through to any host-adaptation path.
    const ref = refStates.get(input as object);
    if (ref) {
      if (ref.runtime !== this) throw new Error("GreeterRef belongs to another runtime");
      if (ref.closed || !bindingIsActive(ref.binding)) throw new Error("GreeterRef is closed");
      return this.acquireCall(ref.binding);
    }

    // Only the generated factory can create an entry in implementationStates.
    // Raw method-shaped values are never structurally adapted at runtime.
    const implementation = implementationStates.get(input as object);
    if (!implementation) throw new Error("value is neither a checked ref nor Greeter.implement result");

    let binding = this.#implementationBindings.get(input as object);
    if (!binding || !bindingIsActive(binding)) {
      binding = {
        id: `runtime-${this.#runtimeId}:host-${++this.#nextBindingId}`,
        host: implementation.host,
        implementationRoot: true,
        refOwners: 0,
        callLeases: 0,
      };
      this.#implementationBindings.set(input as object, binding);
      this.hostBindCount += 1;
    }
    return this.acquireCall(binding);
  }

  createReturnedRefForProbe(host: GreeterHost): GreeterRef {
    const binding: Binding = {
      id: `runtime-${this.#runtimeId}:returned-${++this.#nextBindingId}`,
      host,
      implementationRoot: false,
      refOwners: 0,
      callLeases: 0,
    };
    return GreeterRef.createForRuntime(this, binding);
  }

  async invoke(
    prepared: PreparedGreeter,
    method: "greet" | "label",
    args: readonly unknown[],
  ): Promise<string> {
    const { binding } = prepared;
    this.invocationIdentities.push(binding.id);
    const ctx: HostCallContext = { signal: new AbortController().signal };
    if (method === "greet") return await binding.host.greet(args[0] as string, ctx);

    // The host contract contains no label callback. This counter and result
    // stand in for dispatching the default body stored in BAML.
    this.defaultDispatchCount += 1;
    return `baml-default:${binding.id}`;
  }

  private acquireCall(binding: Binding): PreparedGreeter {
    binding.callLeases += 1;
    let released = false;
    return {
      binding,
      release() {
        if (released) return;
        released = true;
        binding.callLeases -= 1;
      },
    };
  }
}

export const defaultRuntime = new MockRuntime();

async function invokeGreeterInput(
  input: GreeterInput,
  method: "greet" | "label",
  args: readonly unknown[],
  runtime: MockRuntime,
): Promise<string> {
  const prepared = await runtime.prepare(input);
  try {
    return await runtime.invoke(prepared, method, args);
  } finally {
    prepared.release();
  }
}

export async function Welcome_async(
  greeter: GreeterInput,
  name: string,
  runtime: MockRuntime = defaultRuntime,
): Promise<string> {
  return await invokeGreeterInput(greeter, "greet", [name], runtime);
}

function assert(condition: unknown, message: string): asserts condition {
  if (!condition) throw new Error(message);
}

function assertEqual<T>(actual: T, expected: T, message: string): void {
  if (actual !== expected) throw new Error(`${message}: expected ${String(expected)}, got ${String(actual)}`);
}

async function expectReject(action: () => unknown | Promise<unknown>, pattern: RegExp): Promise<void> {
  try {
    await action();
  } catch (error) {
    assert(error instanceof Error && pattern.test(error.message), `unexpected rejection: ${String(error)}`);
    return;
  }
  throw new Error(`expected rejection matching ${pattern}`);
}

const rawHost: GreeterHost = {
  greet: async (name) => `Hello, ${name}!`,
};

if (false) {
  // @ts-expect-error Merely implementing GreeterHost is not GreeterInput.
  await Welcome_async(rawHost, "Ada");
}

const implementation = Greeter.implement(rawHost);
assertEqual(defaultRuntime.hostBindCount, 0, "Greeter.implement performed an eager runtime bind");

assert((await Welcome_async(implementation, "Ada")) === "Hello, Ada!", "factory input direct call failed");
assertEqual(defaultRuntime.hostBindCount, 1, "first generated call did not bind exactly once");
const firstIdentity = defaultRuntime.invocationIdentities.at(-1)!;

assert((await Welcome_async(implementation, "Bo")) === "Hello, Bo!", "repeated input call failed");
assertEqual(defaultRuntime.hostBindCount, 1, "repeated input did not reuse its active registration");
assert(defaultRuntime.invocationIdentities.at(-1) === firstIdentity, "repeated input changed identity");

const label = await implementation.label();
assert(label === `baml-default:${firstIdentity}`, "default label did not use BAML dispatcher result");
assertEqual(defaultRuntime.defaultDispatchCount, 1, "default label dispatcher was not called exactly once");

const independent = Greeter.implement(rawHost);
await Welcome_async(independent, "Cy");
assertEqual(defaultRuntime.hostBindCount, 2, "independent wrapper reused another wrapper's registration");
assert(defaultRuntime.invocationIdentities.at(-1) !== firstIdentity, "independent wrapper reused identity");

const otherRuntime = new MockRuntime();
await Welcome_async(implementation, "Di", otherRuntime);
assertEqual(otherRuntime.hostBindCount, 1, "same wrapper was not independently prepared in another runtime");

const returned = defaultRuntime.createReturnedRefForProbe(rawHost);
const clone = returned.clone();
assert(returned.identityForProbe() === clone.identityForProbe(), "clone did not retain receiver identity");
const bindsBeforeRefs = defaultRuntime.hostBindCount;
returned.close();
await expectReject(() => Welcome_async(returned, "closed"), /GreeterRef is closed/);
assertEqual(defaultRuntime.hostBindCount, bindsBeforeRefs, "closed ref fell through to host adaptation");
assert((await Welcome_async(clone, "Eve")) === "Hello, Eve!", "closing original invalidated clone");
clone.close();
await expectReject(() => Welcome_async(clone, "closed"), /GreeterRef is closed/);

const forged = rawHost as unknown as GreeterInput;
await expectReject(
  () => Welcome_async(forged, "forged"),
  /neither a checked ref nor Greeter\.implement result/,
);

console.log("ergonomic input probe passed");
console.log("factory bind count: construct=0, first call=1, repeated call=1, independent wrapper=2");
console.log("default label dispatched through mock BAML runtime; checked ref clones retained identity independently");
