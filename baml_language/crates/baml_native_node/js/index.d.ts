// Types for the native bridge's runtime. The names are today's bridge's
// (`@boundaryml/baml-bridge`); the ones the native bridge does not implement
// are typed loosely, as they only throw.

export interface BamlErrorDetail {
  value?: unknown;
  bamlTrace?: string[];
  className?: string;
}
export declare class BamlError extends Error {
  readonly value: unknown;
  readonly bamlTrace: string[];
  readonly className: string | undefined;
  constructor(message: string, detail?: BamlErrorDetail);
}
export declare class BamlInvalidArgumentError extends BamlError {}
export declare class BamlClientError extends BamlError {}
export declare class BamlCancelledError extends BamlError {}
export declare class BamlPanic extends BamlError {}
export declare class BamlAbortError extends Error {
  readonly reason: unknown;
  constructor(message: string, options?: { reason?: unknown });
}
export declare function wrapNativeError(err: unknown): BamlError;

export type LazyEntry = () => unknown;
export declare class BamlTypeMap {
  static fromLazyEntries(args: {
    classes: Record<string, LazyEntry>;
    enums: Record<string, LazyEntry>;
    typeAliases: Record<string, LazyEntry>;
    descriptor?: string;
  }): BamlTypeMap;
  readonly descriptor: string | null;
  getClass(fqn: string): unknown;
  getEnum(fqn: string): unknown;
  getTypeAlias(fqn: string): unknown;
  warm(): void;
  captureType(ctor: unknown): string;
  jsTypeToBamlType(ctor: unknown): string;
}
export declare function setTypeMap(m: BamlTypeMap): void;
export declare function getTypeMap(): BamlTypeMap;

export declare function initializeNativeLibrary(path: string, interfaceHash: string): void;

export type Mode = "sync" | "async";
export interface GenericParams {
  typeParams?: readonly string[];
  classTypeParams?: readonly string[];
}
export declare const UNSET: unique symbol;
export declare function defineFunction(
  bamlFqn: string,
  mode: Mode,
  requiredParamNames: readonly string[],
  optionalParamNames?: readonly string[] | undefined,
  generics?: GenericParams | undefined,
): (...args: unknown[]) => unknown;
export declare function defineInstanceFunction(
  bamlFqn: string,
  mode: Mode,
  requiredParamNames: readonly string[],
  optionalParamNames?: readonly string[] | undefined,
  generics?: GenericParams | undefined,
): { bind(self: unknown): (...args: unknown[]) => unknown };

export type BamlTypeToken = any;
export type BamlPrimitiveToken = any;
export type BamlClassCtor = any;
export type BamlInterfaceToken = any;
export type BamlTypeMetadata = any;
export type BamlOptions = any;
export type BamlPromptMessage = any;
export type BamlPromptCallOptions = any;
export type BamlFunctionSpecBuildRequestOptions = any;
export type BamlFunctionSpecCallOptions = any;
export declare const BamlRuntime: any;
export declare class BamlHandle<A = any, B = any> { constructor(...args: any[]); [key: string]: any; static [key: string]: any }
export declare class BamlImage<A = any, B = any> { constructor(...args: any[]); [key: string]: any; static [key: string]: any }
export declare class BamlAudio<A = any, B = any> { constructor(...args: any[]); [key: string]: any; static [key: string]: any }
export declare class BamlVideo<A = any, B = any> { constructor(...args: any[]); [key: string]: any; static [key: string]: any }
export declare class BamlPdf<A = any, B = any> { constructor(...args: any[]); [key: string]: any; static [key: string]: any }
export declare class BamlStream<A = any, B = any> { constructor(...args: any[]); [key: string]: any; static [key: string]: any }
export declare class BamlFunctionSpec<A = any, B = any> { constructor(...args: any[]); [key: string]: any; static [key: string]: any }
export declare class BamlPrompt<A = any, B = any> { constructor(...args: any[]); [key: string]: any; static [key: string]: any }
export declare class BamlReference<A = any, B = any> { constructor(...args: any[]); [key: string]: any; static [key: string]: any }
export declare class BamlType<A = any, B = any> { constructor(...args: any[]); [key: string]: any; static [key: string]: any }
export declare const Never: any;
export declare class TraceUsageError extends Error {}
export declare const FunctionResult: any;
export declare const _ExecutionContext: any;
export declare const getRuntime: any;
export declare const getBridgeRuntimeVersion: any;
export declare const getToolchainVersion: any;
export declare const getVersion: any;
export declare const shutdownRuntime: () => void;
export declare const _seedGenericMediaHandle: any;
export declare const encodeCallArgs: any;
export declare const decodeCallResult: any;
export declare const lowerTypeToWireTy: any;
export declare const reflectType: any;
export declare const initializeRuntime: any;
export declare const initializeRuntimeFromBlob: any;
export declare const newFunctionCall: any;
export declare const cancelFunctionCall: any;
export declare const callFunctionSync: any;
export declare const callFunction: any;
export declare const _currentExecutionContext: any;
export declare const _currentTraceContext: any;
export declare const _currentTraceContextAsync: any;
export declare const _currentCancelToken: any;
export declare const _withExecutionContext: any;
export declare const _invoke: any;
export declare const _invokeSync: any;
export declare const _instrument: any;
export declare const registerCapture: any;
export declare const captureFor: any;
