// A minimal model of the proposed generated TypeScript API. This is a probe,
// not an implementation of the BAML bridge.

declare const clientInputBrand: unique symbol;
declare const decoderInputBrand: unique symbol;
declare const bamlTypeBrand: unique symbol;

type Invariant<T> = (value: T) => T;

export interface HostCallContext {
  readonly signal: AbortSignal;
}

export interface BamlImage {
  readonly kind: "image";
}

export interface Invoice {
  readonly vendor: string;
}

export interface Resume {
  readonly name: string;
}

export interface BamlType<T> {
  readonly id: string;
  readonly validate: (value: unknown) => value is T;
  // A function-valued property is invariant under strictFunctionTypes.
  readonly [bamlTypeBrand]: Invariant<T>;
}

function makeType<T>(id: string, validate: (value: unknown) => value is T): BamlType<T> {
  // The brand is deliberately erased. It is static evidence, not runtime trust.
  return { id, validate } as BamlType<T>;
}

export const types = {
  string: makeType("string", (value): value is string => typeof value === "string"),
  stringOrNumber: makeType(
    "string|number",
    (value): value is string | number => typeof value === "string" || typeof value === "number",
  ),
  Invoice: makeType(
    "Invoice",
    (value): value is Invoice =>
      typeof value === "object" && value !== null && typeof (value as { vendor?: unknown }).vendor === "string",
  ),
};

interface Projection {
  readonly interfaceName: "Client" | "Decoder";
  readonly session: string;
  readonly outputType?: BamlType<unknown>;
  readonly scope?: BridgeScope;
  readonly implementation?: DecoderHost<unknown>;
}

const projections = new WeakMap<object, readonly Projection[]>();

function installProjection(target: object, projection: Projection): void {
  projections.set(target, [...(projections.get(target) ?? []), projection]);
}

function lookupProjection(
  target: object,
  interfaceName: Projection["interfaceName"],
  session: string,
  outputType?: BamlType<unknown>,
): Projection {
  const projection = projections
    .get(target)
    ?.find((candidate) => candidate.interfaceName === interfaceName && candidate.session === session);
  if (!projection) throw new Error(`no checked ${interfaceName} projection for session ${session}`);
  if (projection.scope?.closed) throw new Error("bridge scope is closed");
  if (outputType && projection.outputType?.id !== outputType.id) {
    throw new Error(`associated pin mismatch: expected ${outputType.id}, got ${projection.outputType?.id}`);
  }
  return projection;
}

export interface ClientInput {
  // This non-exported symbol rejects method-shaped host objects statically.
  readonly [clientInputBrand]: true;
}

export interface ClientHost {
  id: (ctx: HostCallContext) => string | Promise<string>;
}

export class ResponsesClient implements ClientInput {
  declare readonly [clientInputBrand]: true;

  constructor(readonly session: string) {
    installProjection(this, { interfaceName: "Client", session });
  }

  async id(): Promise<string> {
    return "responses";
  }
}

export class ClientRef implements ClientInput {
  declare readonly [clientInputBrand]: true;

  private constructor(readonly session: string) {
    installProjection(this, { interfaceName: "Client", session });
  }

  static createForProbe(session: string): ClientRef {
    return new ClientRef(session);
  }

  async id(): Promise<string> {
    return "ref";
  }
}

export function acceptClient(_client: ClientInput): void {}

export function encodeClient(client: ClientInput, session: string): string {
  lookupProjection(client as object, "Client", session);
  return "checked-client-view";
}

export interface DecoderHost<Output> {
  // Output appears only covariantly here, as intended for this producer shape.
  decode(input: BamlImage, ctx: HostCallContext): Output | Promise<Output>;
}

export interface DecoderInput<Output> {
  // Function property, rather than method syntax, makes the pin invariant.
  readonly [decoderInputBrand]: Invariant<Output>;
}

export class BridgeScope {
  #closed = false;
  readonly #calls = new Set<AbortController>();

  startCallForProbe(): { readonly signal: AbortSignal; finish(): void } {
    if (this.#closed) throw new Error("bridge scope is closed");
    const controller = new AbortController();
    this.#calls.add(controller);
    return {
      signal: controller.signal,
      finish: () => this.#calls.delete(controller),
    };
  }

  get closed(): boolean {
    return this.#closed;
  }

  close(): void {
    if (this.#closed) return;
    this.#closed = true;
    for (const controller of this.#calls) controller.abort(new Error("bridge scope closed"));
    this.#calls.clear();
  }
}

export class DecoderRef<Output> implements DecoderInput<Output> {
  declare readonly [decoderInputBrand]: Invariant<Output>;

  private constructor(
    readonly session: string,
    readonly scope: BridgeScope,
    output: BamlType<Output>,
    implementation: DecoderHost<Output>,
  ) {
    installProjection(this, {
      interfaceName: "Decoder",
      session,
      outputType: output as BamlType<unknown>,
      scope,
      implementation: implementation as DecoderHost<unknown>,
    });
  }

  static async bind<Output>(
    implementation: DecoderHost<Output>,
    options: {
      // Output is inferred from/selected for the native host contract; the token
      // must agree instead of widening Output to reconcile two arguments.
      output: BamlType<NoInfer<Output>>;
      scope: BridgeScope;
      session: string;
    },
  ): Promise<DecoderRef<Output>> {
    return new DecoderRef(options.session, options.scope, options.output, implementation);
  }

  async decode(input: BamlImage): Promise<Output> {
    const projection = lookupProjection(this, "Decoder", this.session);
    const call = this.scope.startCallForProbe();
    try {
      const value = await projection.implementation!.decode(input, { signal: call.signal });
      if (!projection.outputType!.validate(value)) {
        throw new Error(`host returned a value outside associated pin ${projection.outputType!.id}`);
      }
      return value as Output;
    } finally {
      call.finish();
    }
  }
}

export class GeneratedStringDecoder implements DecoderInput<string> {
  declare readonly [decoderInputBrand]: Invariant<string>;

  constructor(readonly session: string) {
    installProjection(this, {
      interfaceName: "Decoder",
      session,
      outputType: types.string as BamlType<unknown>,
    });
  }
}

export function readText(_decoder: DecoderInput<string>): void {}

export function encodeDecoder<Output>(
  decoder: DecoderInput<Output>,
  session: string,
  output: BamlType<Output>,
): string {
  lookupProjection(decoder as object, "Decoder", session, output as BamlType<unknown>);
  return "checked-decoder-view";
}

// These two shapes isolate TypeScript's method-parameter bivariance exception.
export interface MethodInvariantAttempt<T> {
  pin(value: T): T;
}

export interface PropertyInvariant<T> {
  pin: (value: T) => T;
}

declare const functionSpecOutput: unique symbol;

export class FunctionSpec<Output> {
  declare private readonly [functionSpecOutput]: Invariant<Output>;

  constructor(
    readonly name: string,
    private readonly execute: () => Output | Promise<Output>,
  ) {}

  runForProbe(): Output | Promise<Output> {
    return this.execute();
  }
}

export class RunResult<Output> {
  constructor(private readonly value: Output) {}

  async get_value(): Promise<Output> {
    return this.value;
  }
}

export class Agent {
  async run<Output>(spec: FunctionSpec<Output>): Promise<RunResult<Output>> {
    return new RunResult(await spec.runForProbe());
  }
}

export function runtimeSymbolCount(value: object): number {
  return Object.getOwnPropertySymbols(value).length;
}
