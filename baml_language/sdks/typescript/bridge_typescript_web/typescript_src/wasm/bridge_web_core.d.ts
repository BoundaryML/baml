/* tslint:disable */
/* eslint-disable */

export function _testHandleTableEntryCount(): number;

export function _testWebFireHostRelease(key: bigint): void;

export function _testWebHostCallableCount(): number;

export function _testWebHostReleaseCallbackInstalled(): boolean;

export function _testWebInFlightHostCallCount(): number;

export function _testWebMissingHostCallableError(key: bigint): Promise<string>;

export function _testWebSyncPendingHostCallableError(key: bigint): string;

export function callFunction(encoded_args: Uint8Array): Promise<Uint8Array>;

export function callFunctionSync(encoded_args: Uint8Array): Uint8Array;

export function cancelFunctionCall(call_id: bigint): boolean;

export function cloneHandle(key: bigint): bigint;

/**
 * Complete an in-flight host call from JS.
 *
 * Exposed to JS as `completeHostCall(callId, isError, content)`.
 *
 * On success (`is_error == 0`), `content` is a protobuf-encoded `InboundValue`
 * (host→engine direction, no type metadata — engine re-validates against the
 * declared return type).
 *
 * On error (`is_error != 0`), `content` is a protobuf-encoded `InboundValue`
 * representing the thrown value. The host bridge SDK wraps native exceptions
 * in a synthetic `Instance` of class `baml.errors.HostCallable` carrying
 * `message` / `class_name` / `language` / `traceback` fields; codegenned
 * BAML errors flow through as their own `Instance` shape. The engine's
 * `materialize_host_throw` runs the declared-throws contract check on the
 * decoded value and either re-injects it as a catchable throw or escalates
 * to a `HostContractViolation` panic.
 *
 * `call_id` is globally unique, so it resolves the originating runtime's
 * pending call unambiguously. Returns `true` when a live entry was removed and
 * completed. An unknown ID returns `false` after a bounded warning, making a
 * Promise settlement racing with cancellation an observable benign stale
 * completion.
 */
export function completeHostCall(call_id: number, is_error: number, content: Uint8Array): boolean;

export function completeWebHostCall(call_id: number, is_error: number, content: Uint8Array): boolean;

export function configureWebSysops(fetch_key: bigint, read_file_sync_key: bigint): void;

/**
 * Configure the workerd-only non-cryptographic `UUIDv4` source before a
 * generated SDK stages bytecode at module scope.
 */
export function configureWorkerdUuidSeed(seed: bigint): void;

export function flushEvents(): void;

export function getBridgeRuntimeVersion(): string;

export function getToolchainVersion(): string;

export function getVersion(): string;

export function init(): void;

export function mediaBase64(key: bigint, handle_type: number): string;

export function mediaFile(key: bigint, handle_type: number): string | undefined;

export function mediaFromBase64(media_kind_value: number, base64: string, mime_type?: string | null): bigint;

export function mediaFromFile(media_kind_value: number, file: string, mime_type?: string | null): bigint;

export function mediaFromUrl(media_kind_value: number, url: string, mime_type?: string | null): bigint;

export function mediaMimeType(key: bigint, handle_type: number): string | undefined;

export function mediaUrl(key: bigint, handle_type: number): string | undefined;

/**
 * Mint a key for a JS-owned opaque value from the same keyspace used by
 * callable host values. Both kinds share one engine handle table.
 */
export function mintHostValueKey(): bigint;

export function mintWebHostValueKey(): bigint;

export function newFunctionCall(): bigint;

/**
 * Register a JS callable in the WASM host-value table and return its key.
 *
 * Exposed to JS as `registerHostCallable(fn) -> bigint`. The key is then
 * embedded in `InboundValue::Handle { key, handleType: HOST_VALUE_CALLABLE }`
 * by the JS encoder. The returned value is a `BigInt` because `u64` does not
 * fit into JS's safe-integer range.
 */
export function registerHostCallable(callable: Function): bigint;

/**
 * Register the JS callback that releases opaque values from the SDK's local
 * registry when the engine drops its last corresponding `HostValueArc`.
 */
export function registerHostValueReleaseCallback(callback: Function): boolean;

export function registerWebHostCallable(callable: Function): bigint;

export function registerWebHostValueReleaseCallback(callback: Function): boolean;

export function releaseHandle(key: bigint): boolean;

/**
 * Remove a callable that was registered but never transferred to the engine.
 */
export function releaseHostCallable(key: bigint): void;

export function releaseWebHostCallable(key: bigint): void;

export function seedFunctionRefHandle(global_index: number): bigint;

export function seedGenericMediaHandle(): bigint;

export function stageRuntimeBytecode(bytecode: Uint8Array, embedded_baml_toml?: string | null): void;

export function stageRuntimeSources(root_path: string, files: any): void;

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
    readonly memory: WebAssembly.Memory;
    readonly cancelFunctionCall: (a: bigint) => number;
    readonly configureWorkerdUuidSeed: (a: bigint) => [number, number];
    readonly flushEvents: () => void;
    readonly getBridgeRuntimeVersion: () => [number, number];
    readonly getToolchainVersion: () => [number, number];
    readonly getVersion: () => [number, number];
    readonly init: () => void;
    readonly newFunctionCall: () => bigint;
    readonly mediaBase64: (a: bigint, b: number) => [number, number, number, number];
    readonly mediaFile: (a: bigint, b: number) => [number, number, number, number];
    readonly mediaFromBase64: (a: number, b: number, c: number, d: number, e: number) => [bigint, number, number];
    readonly mediaFromFile: (a: number, b: number, c: number, d: number, e: number) => [bigint, number, number];
    readonly mediaFromUrl: (a: number, b: number, c: number, d: number, e: number) => [bigint, number, number];
    readonly mediaMimeType: (a: bigint, b: number) => [number, number, number, number];
    readonly mediaUrl: (a: bigint, b: number) => [number, number, number, number];
    readonly _testHandleTableEntryCount: () => [number, number, number];
    readonly cloneHandle: (a: bigint) => [bigint, number, number];
    readonly releaseHandle: (a: bigint) => number;
    readonly seedFunctionRefHandle: (a: number) => [bigint, number, number];
    readonly seedGenericMediaHandle: () => [bigint, number, number];
    readonly callFunction: (a: number, b: number) => any;
    readonly callFunctionSync: (a: number, b: number) => [number, number];
    readonly stageRuntimeBytecode: (a: number, b: number, c: number, d: number) => [number, number];
    readonly stageRuntimeSources: (a: number, b: number, c: any) => [number, number];
    readonly _testWebFireHostRelease: (a: bigint) => void;
    readonly _testWebHostCallableCount: () => number;
    readonly _testWebHostReleaseCallbackInstalled: () => number;
    readonly _testWebInFlightHostCallCount: () => number;
    readonly _testWebMissingHostCallableError: (a: bigint) => any;
    readonly _testWebSyncPendingHostCallableError: (a: bigint) => [number, number];
    readonly completeWebHostCall: (a: number, b: number, c: number, d: number) => number;
    readonly configureWebSysops: (a: bigint, b: bigint) => [number, number];
    readonly mintWebHostValueKey: () => bigint;
    readonly registerWebHostCallable: (a: any) => bigint;
    readonly registerWebHostValueReleaseCallback: (a: any) => number;
    readonly releaseWebHostCallable: (a: bigint) => void;
    readonly completeHostCall: (a: number, b: number, c: number, d: number) => number;
    readonly mintHostValueKey: () => bigint;
    readonly registerHostCallable: (a: any) => bigint;
    readonly registerHostValueReleaseCallback: (a: any) => number;
    readonly releaseHostCallable: (a: bigint) => void;
    readonly cancel_function_call: (a: bigint) => number;
    readonly new_function_call: () => bigint;
    readonly free_buffer: (a: number) => void;
    readonly wasm_bindgen__convert__closures_____invoke__h06cf218e7899498c: (a: number, b: number, c: any) => [number, number];
    readonly wasm_bindgen__convert__closures_____invoke__hc1aeb8686748a7da: (a: number, b: number, c: any, d: any) => void;
    readonly wasm_bindgen__convert__closures_____invoke__h5b34bf2d90ad8f2b: (a: number, b: number) => number;
    readonly __wbindgen_malloc_command_export: (a: number, b: number) => number;
    readonly __wbindgen_realloc_command_export: (a: number, b: number, c: number, d: number) => number;
    readonly __wbindgen_exn_store_command_export: (a: number) => void;
    readonly __externref_table_alloc_command_export: () => number;
    readonly __wbindgen_externrefs: WebAssembly.Table;
    readonly __wbindgen_destroy_closure_command_export: (a: number, b: number) => void;
    readonly __wbindgen_free_command_export: (a: number, b: number, c: number) => void;
    readonly __externref_table_dealloc_command_export: (a: number) => void;
    readonly __wbindgen_start: () => void;
}

export type SyncInitInput = BufferSource | WebAssembly.Module;

/**
 * Instantiates the given `module`, which can either be bytes or
 * a precompiled `WebAssembly.Module`.
 *
 * @param {{ module: SyncInitInput }} module - Passing `SyncInitInput` directly is deprecated.
 *
 * @returns {InitOutput}
 */
export function initSync(module: { module: SyncInitInput } | SyncInitInput): InitOutput;

/**
 * If `module_or_path` is {RequestInfo} or {URL}, makes a request and
 * for everything else, calls `WebAssembly.instantiate` directly.
 *
 * @param {{ module_or_path: InitInput | Promise<InitInput> }} module_or_path - Passing `InitInput` directly is deprecated.
 *
 * @returns {Promise<InitOutput>}
 */
export default function __wbg_init (module_or_path?: { module_or_path: InitInput | Promise<InitInput> } | InitInput | Promise<InitInput>): Promise<InitOutput>;
