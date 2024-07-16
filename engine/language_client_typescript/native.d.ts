/* auto-generated by NAPI-RS */
/* eslint-disable */
export class BamlAudio {
  static fromUrl(url: string): BamlAudio
  static fromBase64(mediaType: string, base64: string): BamlAudio
  isUrl(): boolean
  asUrl(): string
  asBase64(): [string, string]
  toJSON(): any
}

export class BamlImage {
  static fromUrl(url: string): BamlImage
  static fromBase64(mediaType: string, base64: string): BamlImage
  isUrl(): boolean
  asUrl(): string
  asBase64(): [string, string]
  toJSON(): any
}

export class BamlRuntime {
  static fromDirectory(directory: string, envVars: Record<string, string>): BamlRuntime
  static fromFiles(rootPath: string, files: Record<string, string>, envVars: Record<string, string>): BamlRuntime
  createContextManager(): RuntimeContextManager
  callFunction(functionName: string, args: { [string]: any }, ctx: RuntimeContextManager, tb?: TypeBuilder | undefined | null, cb?: ClientRegistry | undefined | null): Promise<FunctionResult>
  streamFunction(functionName: string, args: { [string]: any }, cb: (err: any, param: FunctionResult) => void, ctx: RuntimeContextManager, tb?: TypeBuilder | undefined | null, clientRegistry?: ClientRegistry | undefined | null): FunctionResultStream
  setLogEventCallback(func: (err: any, param: BamlLogEvent) => void): void
  flush(): void
  drainStats(): TraceStats
}

export class BamlSpan {
  static new(runtime: BamlRuntime, functionName: string, args: any, ctx: RuntimeContextManager): BamlSpan
  finish(result: any, ctx: RuntimeContextManager): any
}

export class ClassBuilder {
  field(): FieldType
  property(name: string): ClassPropertyBuilder
}

export class ClassPropertyBuilder {
  setType(fieldType: FieldType): ClassPropertyBuilder
  alias(alias?: string | undefined | null): ClassPropertyBuilder
  description(description?: string | undefined | null): ClassPropertyBuilder
}

export class ClientRegistry {
  constructor()
  addLlmClient(name: string, provider: string, options: { [string]: any }, retryPolicy?: string | undefined | null): void
  setPrimary(primary: string): void
}

export class EnumBuilder {
  value(name: string): EnumValueBuilder
  alias(alias?: string | undefined | null): EnumBuilder
  field(): FieldType
}

export class EnumValueBuilder {
  alias(alias?: string | undefined | null): EnumValueBuilder
  skip(skip?: boolean | undefined | null): EnumValueBuilder
  description(description?: string | undefined | null): EnumValueBuilder
}

export class FieldType {
  list(): FieldType
  optional(): FieldType
}

export class FunctionResult {
  parsed(): any
}

export class FunctionResultStream {
  onEvent(func: (err: any, param: FunctionResult) => void): void
  done(rctx: RuntimeContextManager): Promise<FunctionResult>
}

export class RuntimeContextManager {
  upsertTags(tags: any): void
  deepClone(): RuntimeContextManager
}

export class TraceStats {
  get failed(): number
  get started(): number
  get finalized(): number
  get submitted(): number
  get sent(): number
  get done(): number
  toJson(): string
}

export class TypeBuilder {
  constructor()
  getEnum(name: string): EnumBuilder
  getClass(name: string): ClassBuilder
  list(inner: FieldType): FieldType
  optional(inner: FieldType): FieldType
  string(): FieldType
  int(): FieldType
  float(): FieldType
  bool(): FieldType
  null(): FieldType
}

export interface BamlLogEvent {
  metadata: LogEventMetadata
  prompt?: string
  rawOutput?: string
  parsedOutput?: string
  startTime: string
}

export function invoke_runtime_cli(params: Array<string>): void

export interface LogEventMetadata {
  eventId: string
  parentId?: string
  rootEventId: string
}
