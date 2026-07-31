import {
  type BqfFrame,
  BqfFrameKind,
  bqfColumn,
  decodeBqf1,
} from './observe-bqf';

export type ObserveQuery =
  | {
      kind: 'runs';
      limit?: number;
      maxBytes?: number;
    }
  | {
      kind: 'timeline';
      boundaryId: string;
      viewport: ObserveViewport;
    }
  | {
      kind: 'leftHeavy';
      boundaryId: string;
      pixelWidth: number;
      maxBytes?: number;
    }
  | {
      kind: 'sandwich';
      boundaryId: string;
      functionId: number;
      callerDepth?: number;
      calleeDepth?: number;
      maxRows?: number;
      maxBytes?: number;
    }
  | {
      kind: 'valueRefs';
      boundaryId: string;
      maxRows?: number;
      maxBytes?: number;
    }
  | {
      kind: 'valueDag';
      boundaryId: string;
      rootCid: string;
      maxDepth?: number;
      maxNodes?: number;
      maxBytes?: number;
    }
  | {
      kind: 'valueDiff';
      leftBoundaryId: string;
      leftRootCid: string;
      rightBoundaryId: string;
      rightRootCid: string;
      maxNodes?: number;
      maxBytes?: number;
    }
  | {
      kind: 'search';
      boundaryId: string;
      text: string;
      maxRows?: number;
      maxBytes?: number;
    }
  | {
      kind: 'diff';
      leftBoundaryId: string;
      rightBoundaryId: string;
      maxRows?: number;
      maxBytes?: number;
    }
  | {
      kind: 'bql';
      source: string;
      maxRows?: number;
      maxBytes?: number;
      cursor?: string;
      snapshot?: string;
      params?: Readonly<Record<string, string>>;
    }
  | {
      kind: 'bqlSchema';
      maxBytes?: number;
    };

export type ObserveLiveQuery = Exclude<
  ObserveQuery,
  { kind: 'bql' | 'bqlSchema' }
>;

export type ObserveViewport = {
  startNs: number;
  endNs: number;
  pixelWidth: number;
  lanes: number;
  maxBytes: number;
};

export type ObserveRun = {
  boundaryId: string;
  createdMs: number;
  target: string;
  state: number;
  hasSnapshot: boolean;
  tornTail: boolean;
};

export type LeftHeavyRow = {
  nodeId: number;
  parentRow: number;
  functionId: number;
  depth: number;
  extentPpm: number;
  totalNs: bigint;
  selfNs: bigint;
  awaitNs: bigint;
  calls: bigint;
  errors: bigint;
  syntheticSmaller: boolean;
};

export type SandwichRow = {
  direction: 1 | 2 | 3;
  depth: number;
  functionId: number;
  calls: bigint;
  errors: bigint;
  totalNs: bigint;
  selfNs: bigint;
  awaitingNs: bigint;
};

export type ObserveValueRef = {
  id: string;
  role: string;
  availability: number;
  originalSizeBytes: bigint | null;
  retainedSizeBytes: bigint | null;
  diagnostic: string | null;
  promotionTrigger: string | null;
  rootCid: string | null;
  logicalLength: bigint | null;
};

export type ObserveValueDagRowKind = 1 | 2 | 3 | 4 | 5 | 6 | 7;

export type ObserveValueDagRow = {
  kind: ObserveValueDagRowKind;
  primaryCid: string | null;
  secondaryCid: string | null;
  depth: number;
  ordinal: number;
  logicalLength: bigint | null;
  equal: boolean;
  canonicalLoaded: boolean;
};

export type ObserveSearchRow = {
  functionId: number;
  definitionKey: string;
  fqn: string;
  contexts: number;
  relevance: number;
  calls: bigint;
  errors: bigint;
  totalNs: bigint;
  selfNs: bigint;
  awaitingNs: bigint;
};

export type ObserveDiffRow = {
  definitionKey: string;
  fqn: string;
  leftFunctionId: number | null;
  rightFunctionId: number | null;
  presence: 0 | 1 | 2;
  definitionChanged: boolean;
  deltaCalls: bigint;
  deltaErrors: bigint;
  deltaTotalNs: bigint;
  deltaSelfNs: bigint;
  deltaAwaitingNs: bigint;
};

export type BqlQueryMeta = {
  complete: boolean;
  watermarks: readonly {
    wall_epoch_ns: number;
    drained_through_ts_ns: number;
    events_drained: number;
    durable_kind: number;
    reason: number;
  }[];
  capture_loss: readonly {
    kind: string;
    timestamp_ns: number;
    node_id: number | null;
    count: number;
    message: string;
  }[];
  sources_consulted: readonly number[];
  truncated: boolean;
  next_cursor: string | null;
  warnings: readonly string[];
  snapshot: string;
};

export type BqlQueryResult = {
  name: string | null;
  kind: string;
  columns: readonly string[];
  rows: readonly Record<string, unknown>[];
  meta: BqlQueryMeta;
};

export type BqlStageArgument = {
  name: string;
  value_type: string;
  required: boolean;
  default: string | null;
  units: string | null;
  enum_values: readonly string[];
  example: string;
};

export type BqlStageSpec = {
  name: string;
  category: string;
  inputs: readonly string[];
  output: string;
  preserves_input: boolean;
  arguments: readonly BqlStageArgument[];
  availability: 'implemented' | 'typed_unavailable';
  description: string;
};

export type BqlSchema = {
  version: number;
  default_limit: number;
  hard_max_rows: number;
  hard_max_bytes: number;
  set_kinds: readonly string[];
  stages: readonly BqlStageSpec[];
  fields: readonly {
    set: string;
    name: string;
    value_type: string;
    units: string | null;
    enum_values: readonly string[];
    id_drilldown: string | null;
  }[];
};

export type ObserveSubscription = {
  setViewport(viewport: ObserveViewport): void;
  unsubscribe(): void;
};

export class WsObserveClient {
  private readonly url: string;
  private ws: WebSocket | null = null;
  private connecting: Promise<void> | null = null;
  private nextId = 1;
  private pending = new Map<
    number,
    { resolve(frame: BqfFrame): void; reject(error: Error): void }
  >();
  private subscriptions = new Map<number, (frame: BqfFrame) => void>();

  constructor(url = defaultObserveUrl()) {
    this.url = url;
  }

  async query(query: ObserveQuery): Promise<BqfFrame> {
    await this.connect();
    const requestId = this.nextId++;
    const result = new Promise<BqfFrame>((resolve, reject) => {
      this.pending.set(requestId, { reject, resolve });
    });
    this.send({ query, requestId, type: 'query' });
    return result;
  }

  async subscribe(
    query: ObserveLiveQuery,
    onFrame: (frame: BqfFrame) => void,
    rateHz = 10,
  ): Promise<ObserveSubscription> {
    await this.connect();
    const subscriptionId = this.nextId++;
    this.subscriptions.set(subscriptionId, onFrame);
    this.send({ query, rateHz, subscriptionId, type: 'sub' });
    return {
      setViewport: (viewport) => {
        this.send({ subscriptionId, type: 'setViewport', viewport });
      },
      unsubscribe: () => {
        this.subscriptions.delete(subscriptionId);
        this.send({ subscriptionId, type: 'unsub' });
      },
    };
  }

  close(): void {
    this.ws?.close();
    this.ws = null;
    this.connecting = null;
  }

  private connect(): Promise<void> {
    if (this.ws?.readyState === WebSocket.OPEN) return Promise.resolve();
    if (this.connecting != null) return this.connecting;
    this.connecting = new Promise<void>((resolve, reject) => {
      const ws = new WebSocket(this.url);
      ws.binaryType = 'arraybuffer';
      ws.onopen = () => resolve();
      ws.onerror = () => reject(new Error(`failed to connect to ${this.url}`));
      ws.onclose = () => {
        const error = new Error('observability connection closed');
        for (const pending of this.pending.values()) pending.reject(error);
        this.pending.clear();
        this.subscriptions.clear();
        this.ws = null;
        this.connecting = null;
      };
      ws.onmessage = (event) => {
        if (typeof event.data === 'string') {
          const control = JSON.parse(event.data) as {
            type: string;
            requestId?: number;
            subscriptionId?: number;
            code?: string;
            message?: string;
          };
          if (control.type === 'error') {
            const error = new Error(
              control.message ?? 'observability query failed',
            );
            error.name = control.code ?? 'ObserveQueryError';
            if (control.requestId != null) {
              this.pending.get(control.requestId)?.reject(error);
              this.pending.delete(control.requestId);
            }
          }
          return;
        }
        const frame = decodeBqf1(event.data as ArrayBuffer);
        const id = Number(frame.requestId);
        const pending = this.pending.get(id);
        if (pending != null) {
          this.pending.delete(id);
          pending.resolve(frame);
          return;
        }
        const subscriber = this.subscriptions.get(id);
        if (subscriber != null) {
          // Ack after synchronous snapshot delivery. React consumers retain
          // decoded values, not the transport's mutable websocket state.
          subscriber(frame);
          this.send({ subscriptionId: id, type: 'ack' });
        }
      };
      this.ws = ws;
    });
    return this.connecting;
  }

  private send(message: unknown): void {
    if (this.ws?.readyState !== WebSocket.OPEN) {
      throw new Error('observability websocket is not connected');
    }
    this.ws.send(JSON.stringify(message));
  }
}

export function decodeRunsFrame(frame: BqfFrame): ObserveRun[] {
  if (frame.kind !== BqfFrameKind.Runs) throw new Error('expected Runs frame');
  const boundaryIds = bqfColumn<readonly string[]>(frame, 1);
  const createdMs = bqfColumn<BigUint64Array>(frame, 2);
  const targets = bqfColumn<readonly string[]>(frame, 3);
  const states = bqfColumn<Uint8Array>(frame, 4);
  const snapshots = bqfColumn<Uint8Array>(frame, 5);
  const torn = bqfColumn<Uint8Array>(frame, 6);
  return Array.from({ length: frame.rowCount }, (_, index) => ({
    boundaryId: boundaryIds[index] ?? '',
    createdMs: Number(createdMs[index] ?? 0n),
    hasSnapshot: snapshots[index] === 1,
    state: states[index] ?? 0,
    target: targets[index] ?? '',
    tornTail: torn[index] === 1,
  }));
}

export function decodeLeftHeavyFrame(frame: BqfFrame): LeftHeavyRow[] {
  if (frame.kind !== BqfFrameKind.LeftHeavy) {
    throw new Error('expected Left Heavy frame');
  }
  const nodeIds = bqfColumn<Uint32Array>(frame, 1);
  const parentRows = bqfColumn<Uint32Array>(frame, 2);
  const functionIds = bqfColumn<Uint32Array>(frame, 3);
  const depths = bqfColumn<Uint16Array>(frame, 4);
  const extents = bqfColumn<Uint32Array>(frame, 5);
  const total = bqfColumn<BigUint64Array>(frame, 6);
  const self = bqfColumn<BigUint64Array>(frame, 7);
  const awaiting = bqfColumn<BigUint64Array>(frame, 8);
  const calls = bqfColumn<BigUint64Array>(frame, 9);
  const errors = bqfColumn<BigUint64Array>(frame, 10);
  const synthetic = bqfColumn<Uint8Array>(frame, 11);
  return Array.from({ length: frame.rowCount }, (_, index) => ({
    awaitNs: awaiting[index] ?? 0n,
    calls: calls[index] ?? 0n,
    depth: depths[index] ?? 0,
    errors: errors[index] ?? 0n,
    extentPpm: extents[index] ?? 0,
    functionId: functionIds[index] ?? 0,
    nodeId: nodeIds[index] ?? 0,
    parentRow: parentRows[index] ?? 0,
    selfNs: self[index] ?? 0n,
    syntheticSmaller: synthetic[index] === 1,
    totalNs: total[index] ?? 0n,
  }));
}

export function decodeSandwichFrame(frame: BqfFrame): SandwichRow[] {
  if (frame.kind !== BqfFrameKind.Sandwich) {
    throw new Error('expected Sandwich frame');
  }
  const directions = bqfColumn<Uint8Array>(frame, 1);
  const depths = bqfColumn<Uint16Array>(frame, 2);
  const functionIds = bqfColumn<Uint32Array>(frame, 3);
  const calls = bqfColumn<BigUint64Array>(frame, 10);
  const errors = bqfColumn<BigUint64Array>(frame, 11);
  const totalNs = bqfColumn<BigUint64Array>(frame, 12);
  const selfNs = bqfColumn<BigUint64Array>(frame, 13);
  const awaitingNs = bqfColumn<BigUint64Array>(frame, 14);
  return Array.from({ length: frame.rowCount }, (_, index) => ({
    awaitingNs: awaitingNs[index] ?? 0n,
    calls: calls[index] ?? 0n,
    depth: depths[index] ?? 0,
    direction: (directions[index] ?? 2) as 1 | 2 | 3,
    errors: errors[index] ?? 0n,
    functionId: functionIds[index] ?? 0,
    selfNs: selfNs[index] ?? 0n,
    totalNs: totalNs[index] ?? 0n,
  }));
}

export function decodeValueRefsFrame(frame: BqfFrame): ObserveValueRef[] {
  if (frame.kind !== BqfFrameKind.ValueRefs) {
    throw new Error('expected Value Refs frame');
  }
  const ids = bqfColumn<readonly string[]>(frame, 1);
  const roles = bqfColumn<readonly string[]>(frame, 2);
  const availability = bqfColumn<Uint8Array>(frame, 3);
  const original = bqfColumn<BigUint64Array>(frame, 4);
  const retained = bqfColumn<BigUint64Array>(frame, 5);
  const diagnostics = bqfColumn<readonly string[]>(frame, 6);
  const triggers = bqfColumn<readonly string[]>(frame, 7);
  const cids = bqfColumn<readonly string[]>(frame, 8);
  const logical = bqfColumn<BigUint64Array>(frame, 9);
  const missing = 0xffffffffffffffffn;
  return Array.from({ length: frame.rowCount }, (_, index) => ({
    availability: availability[index] ?? 0,
    diagnostic: diagnostics[index] ? diagnostics[index] : null,
    id: ids[index] ?? '',
    logicalLength:
      (logical[index] ?? missing) === missing ? null : (logical[index] ?? null),
    originalSizeBytes:
      (original[index] ?? missing) === missing
        ? null
        : (original[index] ?? null),
    promotionTrigger: triggers[index] ? triggers[index] : null,
    retainedSizeBytes:
      (retained[index] ?? missing) === missing
        ? null
        : (retained[index] ?? null),
    role: roles[index] ?? 'value',
    rootCid: cids[index] ? cids[index] : null,
  }));
}

export function decodeValueDagFrame(frame: BqfFrame): ObserveValueDagRow[] {
  if (frame.kind !== BqfFrameKind.ValueDag) {
    throw new Error('expected Value DAG frame');
  }
  const kinds = bqfColumn<Uint8Array>(frame, 1);
  const primary = bqfColumn<readonly string[]>(frame, 2);
  const secondary = bqfColumn<readonly string[]>(frame, 3);
  const depths = bqfColumn<Uint16Array>(frame, 4);
  const ordinals = bqfColumn<Uint32Array>(frame, 5);
  const logical = bqfColumn<BigUint64Array>(frame, 6);
  const equal = bqfColumn<Uint8Array>(frame, 7);
  const canonicalLoaded = bqfColumn<Uint8Array>(frame, 8);
  const missing = 0xffffffffffffffffn;
  return Array.from({ length: frame.rowCount }, (_, index) => ({
    canonicalLoaded: canonicalLoaded[index] === 1,
    depth: depths[index] ?? 0,
    equal: equal[index] === 1,
    kind: (kinds[index] ?? 1) as ObserveValueDagRowKind,
    logicalLength:
      (logical[index] ?? missing) === missing ? null : (logical[index] ?? null),
    ordinal: ordinals[index] ?? 0,
    primaryCid: primary[index] ? primary[index] : null,
    secondaryCid: secondary[index] ? secondary[index] : null,
  }));
}

export function decodeSearchFrame(frame: BqfFrame): ObserveSearchRow[] {
  if (frame.kind !== BqfFrameKind.Search) {
    throw new Error('expected Search frame');
  }
  const functionIds = bqfColumn<Uint32Array>(frame, 1);
  const definitionKeys = bqfColumn<readonly string[]>(frame, 2);
  const fqns = bqfColumn<readonly string[]>(frame, 3);
  const contexts = bqfColumn<Uint32Array>(frame, 4);
  const relevance = bqfColumn<Uint8Array>(frame, 5);
  const calls = bqfColumn<BigUint64Array>(frame, 10);
  const errors = bqfColumn<BigUint64Array>(frame, 11);
  const totalNs = bqfColumn<BigUint64Array>(frame, 12);
  const selfNs = bqfColumn<BigUint64Array>(frame, 13);
  const awaitingNs = bqfColumn<BigUint64Array>(frame, 14);
  return Array.from({ length: frame.rowCount }, (_, index) => ({
    awaitingNs: awaitingNs[index] ?? 0n,
    calls: calls[index] ?? 0n,
    contexts: contexts[index] ?? 0,
    definitionKey: definitionKeys[index] ?? '',
    errors: errors[index] ?? 0n,
    fqn: fqns[index] ?? '',
    functionId: functionIds[index] ?? 0,
    relevance: relevance[index] ?? 0,
    selfNs: selfNs[index] ?? 0n,
    totalNs: totalNs[index] ?? 0n,
  }));
}

export function decodeDiffFrame(frame: BqfFrame): ObserveDiffRow[] {
  if (frame.kind !== BqfFrameKind.Diff) {
    throw new Error('expected Diff frame');
  }
  const definitionKeys = bqfColumn<readonly string[]>(frame, 1);
  const fqns = bqfColumn<readonly string[]>(frame, 2);
  const leftFunctionIds = bqfColumn<Uint32Array>(frame, 3);
  const rightFunctionIds = bqfColumn<Uint32Array>(frame, 4);
  const presence = bqfColumn<Uint8Array>(frame, 5);
  const definitionChanged = bqfColumn<Uint8Array>(frame, 6);
  const deltaCalls = bqfColumn<BigInt64Array>(frame, 10);
  const deltaErrors = bqfColumn<BigInt64Array>(frame, 11);
  const deltaTotalNs = bqfColumn<BigInt64Array>(frame, 12);
  const deltaSelfNs = bqfColumn<BigInt64Array>(frame, 13);
  const deltaAwaitingNs = bqfColumn<BigInt64Array>(frame, 14);
  const missingFunction = 0xffffffff;
  return Array.from({ length: frame.rowCount }, (_, index) => ({
    definitionChanged: definitionChanged[index] === 1,
    definitionKey: definitionKeys[index] ?? '',
    deltaAwaitingNs: deltaAwaitingNs[index] ?? 0n,
    deltaCalls: deltaCalls[index] ?? 0n,
    deltaErrors: deltaErrors[index] ?? 0n,
    deltaSelfNs: deltaSelfNs[index] ?? 0n,
    deltaTotalNs: deltaTotalNs[index] ?? 0n,
    fqn: fqns[index] ?? '',
    leftFunctionId:
      (leftFunctionIds[index] ?? missingFunction) === missingFunction
        ? null
        : (leftFunctionIds[index] ?? null),
    presence: (presence[index] ?? 0) as 0 | 1 | 2,
    rightFunctionId:
      (rightFunctionIds[index] ?? missingFunction) === missingFunction
        ? null
        : (rightFunctionIds[index] ?? null),
  }));
}

export function decodeBqlFrame(frame: BqfFrame): BqlQueryResult[] {
  if (frame.kind !== BqfFrameKind.Query) {
    throw new Error('expected Query frame');
  }
  const names = bqfColumn<readonly string[]>(frame, 1);
  const kinds = bqfColumn<readonly string[]>(frame, 2);
  const columns = bqfColumn<readonly string[]>(frame, 3);
  const rows = bqfColumn<readonly string[]>(frame, 4);
  const metadata = bqfColumn<readonly string[]>(frame, 5);
  return Array.from({ length: frame.rowCount }, (_, index) => ({
    columns: parseJson<readonly string[]>(
      columns[index] ?? '[]',
      'BQL result columns',
    ),
    kind: parseJson<string>(kinds[index] ?? '"table"', 'BQL result kind'),
    meta: parseJson<BqlQueryMeta>(
      metadata[index] ?? '{}',
      'BQL result metadata',
    ),
    name: names[index] ? names[index] : null,
    rows: parseJson<readonly Record<string, unknown>[]>(
      rows[index] ?? '[]',
      'BQL result rows',
    ),
  }));
}

export function decodeBqlSchemaFrame(frame: BqfFrame): BqlSchema {
  const [result] = decodeBqlFrame(frame);
  if (result == null || result.name !== 'schema' || result.kind !== 'schema') {
    throw new Error('expected BQL schema result');
  }
  const schema = result.rows[0];
  if (schema == null || typeof schema !== 'object') {
    throw new Error('BQL schema result is empty');
  }
  return schema as unknown as BqlSchema;
}

function parseJson<T>(source: string, label: string): T {
  try {
    return JSON.parse(source) as T;
  } catch (cause) {
    throw new Error(
      `invalid ${label}: ${cause instanceof Error ? cause.message : String(cause)}`,
    );
  }
}

function defaultObserveUrl(): string {
  const url = new URL('/api/obs', window.location.href);
  url.protocol = url.protocol === 'https:' ? 'wss:' : 'ws:';
  return url.toString();
}
